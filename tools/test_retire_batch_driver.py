from contextlib import closing
import fcntl
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from retire_queue import Queue

import retire as r
import retire_batch_driver as driver


class QueueStub:
    def __init__(self, progress):
        self.value = progress
        self.rejected = []
        self.ready = []
        self.superseded = []
    def get_jobs(self, status, limit=None):
        rows = {'integration-rejected': self.rejected, 'provisional-pass': self.ready,
                'superseded': self.superseded}.get(status, [])
        return rows[:limit]
    def progress(self):
        return self.value
    def audit(self):
        return []


def progress(pending=1, running=0, ready=0, failed=0, checkpoint='parent'):
    counts = {'pending': pending, 'running': running, 'provisional-pass': ready, 'failed': failed}
    return {'checkpoint': checkpoint, 'counts': counts, 'job_counts': dict(counts)}


class RecoveredCohortTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.queue = Queue(Path(self.temp.name)/'queue.sqlite')
        self.queue.add_checkpoint('head', 'proof')

    def prepare(self, names):
        self.queue.add_jobs([{'id': name, 'rules': ['rule_'+name], 'kind': 'author'} for name in names])
        with closing(self.queue._connect()) as db:
            db.execute('BEGIN IMMEDIATE')
            for name in names:
                db.execute('INSERT INTO attempts(id,job_id,worker,base_commit) VALUES(?,?,?,?)',
                           ('attempt_'+name, name, 'worker', 'head'))
                db.execute("UPDATE jobs SET status='provisional-pass',attempt_id=? WHERE id=?",
                           ('attempt_'+name, name))
            db.commit()

    def event(self, name, manifest='batch-a', checkpoint='parent', attempt=None, kind='readmitted-native-batch'):
        with closing(self.queue._connect()) as db:
            db.execute('BEGIN IMMEDIATE')
            self.queue._event(db, kind, {'job_id': name, 'attempt_id': attempt or 'attempt_'+name,
                                        'manifest': manifest, 'checkpoint': checkpoint})
            db.commit()

    def test_groups_by_current_attempt_and_latest_readmission_not_fresh_work(self):
        self.prepare(['a', 'b', 'c', 'fresh', 'stale'])
        self.event('a', 'batch-old', kind='readmitted-batch')
        self.event('b', 'batch-b', kind='readmitted-library-batch')
        self.event('c', 'batch-b', kind='readmitted-native-exit-one-batch')
        self.event('a', 'batch-b')
        self.event('stale', 'batch-old', attempt='previous-attempt')
        self.assertEqual(driver.recovered_cohort(self.queue, 'head'),
                         {'manifest': 'batch-b', 'checkpoint': 'parent', 'jobs': ['b', 'c', 'a']})
        with self.assertRaisesRegex(ValueError, 'checkpoint changed'):
            driver.recovered_cohort(self.queue, 'stale-head')

    def test_same_manifest_different_checkpoint_is_a_separate_group(self):
        self.prepare(['a', 'b', 'c'])
        self.event('a', 'same', 'first')
        self.event('b', 'same', 'second')
        self.event('c', 'same', 'first')
        self.assertEqual(driver.recovered_cohort(self.queue, 'head'),
                         {'manifest': 'same', 'checkpoint': 'first', 'jobs': ['a', 'c']})

    def test_large_cohort_is_capped_without_mixing_other_groups(self):
        names = [f'j{i:03d}' for i in range(99)]
        self.prepare(names)
        for name in names:
            self.event(name)
        self.assertEqual(driver.recovered_cohort(self.queue, 'head'),
                         {'manifest': 'batch-a', 'checkpoint': 'parent', 'jobs': names[:96]})

    def test_matching_malformed_event_is_not_silently_mixed(self):
        self.prepare(['a'])
        self.event('a', manifest='')
        with self.assertRaisesRegex(ValueError, 'cohort identity'):
            driver.recovered_cohort(self.queue, 'head')

    def test_mechanical_lane_uses_real_queue_fifo_and_exact_threshold(self):
        names = ['a0', 'm0', 'a1', 'm1', 'm2', 'm3', 'a2']
        self.queue.add_jobs([{'id': name, 'rules': ['rule_'+name],
                              'kind': 'mechanical' if name.startswith('m') else 'author'} for name in names])
        with closing(self.queue._connect()) as db:
            db.execute("UPDATE jobs SET status='provisional-pass'")
        self.assertEqual(driver.mechanical_lane(self.queue, 4),
                         {'lane': 'mechanical', 'jobs': ['m0', 'm1', 'm2', 'm3'], 'combine': True})
        self.assertEqual(driver.mechanical_lane(self.queue, 5),
                         {'lane': 'mechanical-first', 'jobs': ['m0', 'm1', 'm2', 'm3', 'a0'], 'combine': False})

    def test_union_opt_in_dispatches_only_the_current_attempt_recovery_group(self):
        self.prepare(['fresh', 'a', 'b', 'stale', 'other'])
        self.event('a', 'isolated'); self.event('b', 'isolated')
        self.event('stale', 'isolated', attempt='previous-attempt'); self.event('other', 'later')
        run = Path(self.temp.name); runner = run/'runners/test'; runner.mkdir(parents=True)
        r.write(run/'config.json', {'first_batch_size': 4, 'batch_size': 4, 'initial_commit': 'head',
                                   'batch_wait_seconds': 900, 'recovered_cohorts_first': True,
                                   'recovered_union_trial': True})
        with closing(self.queue._connect()) as db:
            for name in ('fresh', 'a', 'b', 'stale', 'other'):
                report = run/(name+'.json'); r.write(report, {'identity': name})
                result = {'status': 'provisional-pass', 'evidence': {'kind': 'author', 'run': str(run/name),
                                                                   'report': str(report)}}
                db.execute('UPDATE attempts SET result=? WHERE id=?', (json.dumps(result), 'attempt_'+name))
        def guarded_batch(selected_run, ids, *, combine_selected):
            self.assertEqual(ids, ['a', 'b']); self.assertTrue(combine_selected)
            proposals = driver.dispatch.proposals(selected_run, 4, ids)
            self.assertEqual([(p['job_id'], p['attempt_id'], p['report']['identity']) for p in proposals],
                             [('a', 'attempt_a', 'a'), ('b', 'attempt_b', 'b')])
            raise RuntimeError('guarded staging reached')
        with patch.object(driver, '_runner', return_value=runner), \
                patch.object(driver.dispatch, '_batch', side_effect=guarded_batch) as batch, \
                self.assertRaisesRegex(RuntimeError, 'guarded staging reached'):
            driver.run_driver(run, runner)
        batch.assert_called_once_with(run, ['a', 'b'], combine_selected=True)

    def test_numeric_job_identity_is_not_coerced_to_a_string(self):
        self.prepare(['123'])
        with closing(self.queue._connect()) as db:
            db.execute('BEGIN IMMEDIATE')
            self.queue._event(db, 'readmitted-batch', {'job_id': 123, 'attempt_id': 'attempt_123',
                                                        'manifest': 'batch', 'checkpoint': 'parent'})
            db.commit()
        with self.assertRaisesRegex(ValueError, 'job identity'):
            driver.recovered_cohort(self.queue, 'head')


class DriverTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.run = Path(self.temp.name)
        self.runner = self.run/'runners/v1'
        self.runner.mkdir(parents=True)
        self.queue = QueueStub(progress())
        r.write(self.run/'config.json', {'first_batch_size': 4, 'batch_size': 4,
                                       'initial_commit': 'parent', 'batch_wait_seconds': 900})
        self.validate = patch.object(driver, '_runner', return_value=self.runner)
        self.create_queue = patch.object(driver, 'Queue', return_value=self.queue)
        self.validator = self.validate.start()
        self.create_queue.start()
        self.addCleanup(self.validate.stop)
        self.addCleanup(self.create_queue.stop)

    def outcome(self, *, validation='passed', drain=False):
        def call(run):
            self.assertEqual(len(r._LEASES), 1)
            with (self.run/'batch.lock').open('a') as other:
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(other, fcntl.LOCK_EX | fcntl.LOCK_NB)
            folder = self.run/'.cache/batches/id'
            folder.mkdir(parents=True)
            r.write(folder/'manifest.json', {'id': 'id', 'base_commit': 'parent', 'status': 'provisional', 'commit': 'candidate',
                                             'jobs': [{'job_id': 'job', 'attempt_id': 'attempt', 'status': 'included'}]})
            r.write(folder/'validation.json', {'status': validation, 'candidate': 'candidate', 'source_checkpoint': 'parent',
                                               'reason': 'unexpected infrastructure error' if validation == 'failed' else None})
            if validation == 'passed':
                self.queue.value = progress(pending=0, ready=0, checkpoint='candidate')
                r.write(self.run/'publication.json', {'status': 'integrated'})
            if drain:
                r.write(self.run/'batch-driver-drain.json', {'reason': 'test'})
        return call

    def test_parse_screen_semantic_failure_is_a_rejected_batch(self):
        folder = self.run/'.cache/batches/id'
        folder.mkdir(parents=True)
        job = {'job_id': 'job', 'attempt_id': 'attempt', 'status': 'included'}
        r.write(folder/'manifest.json', {'id': 'id', 'base_commit': 'parent', 'status': 'provisional',
                                        'commit': 'candidate', 'jobs': [job]})
        validation = {'status': 'failed', 'candidate': 'candidate', 'source_checkpoint': 'parent',
                      'reason': 'full allowed-population parse screen failed'}
        r.write(folder/'validation.json', validation)
        self.queue.rejected = [{'id': 'job', 'attempt_id': 'attempt',
                                'adjudication': {'evidence': {'manifest': 'id', 'validation': validation}}}]
        result = driver._batch_observation(self.run, set(), self.queue, 'parent')
        self.assertEqual(result['outcome'], 'rejected')
        self.assertEqual(result['reason'], validation['reason'])

    def test_verified_retry_preserves_a_staging_rejection_during_validation(self):
        folder = self.run/'.cache/batches/id'
        folder.mkdir(parents=True)
        jobs = [{'job_id':'included','attempt_id':'first','status':'included'},
                {'job_id':'retry','attempt_id':'second','status':'integration-rejected'}]
        r.write(folder/'manifest.json', {'id':'id','base_commit':'parent','status':'provisional',
                                         'commit':'candidate','jobs':jobs})
        validation = {'status':'failed','candidate':'candidate','source_checkpoint':'parent',
                      'reason':'full allowed-population parse screen failed'}
        r.write(folder/'validation.json', validation)
        self.queue.rejected = [{'id':'included','attempt_id':'first',
                                'adjudication':{'evidence':{'manifest':'id','validation':validation}}}]
        event = {'job_id':'retry','attempt_id':'second','checkpoint':'parent',
                 'previous_status':'integration-rejected',
                 'previous_adjudication':{'status':'integration-rejected','checkpoint':None,
                                          'evidence':{'manifest':'id','reason':'merge or staging rejected'}}}
        for kind in ('retry-reviewed-repair', 'retry-staging-reviewed-repair'):
            with self.subTest(kind=kind):
                self.queue.audit = lambda: [{'kind': kind, 'data': event}]
                self.assertEqual(driver._batch_observation(self.run, set(), self.queue, 'parent')['outcome'],
                                 'rejected')
                with patch.dict(event['previous_adjudication']['evidence'], manifest='another-batch'):
                    with self.assertRaisesRegex(ValueError, 'disposition is missing or differs'):
                        driver._batch_observation(self.run, set(), self.queue, 'parent')

    def test_recorded_readmission_preserves_batch_observation(self):
        folder = self.run/'.cache/batches/id'
        folder.mkdir(parents=True)
        r.write(folder/'manifest.json', {'id': 'id', 'base_commit': 'parent', 'status': 'provisional',
                                         'commit': 'candidate', 'jobs': [{'job_id': 'one', 'status': 'included',
                                                                           'attempt_id': 'attempt'}]})
        for reason, event_kind, evidence_kind in (
                (driver.PRINT_ONLY_REJECTION, 'readmitted-native-batch', 'isolated-full-check-revalidation'),
                (driver.PARSE_SCREEN_REJECTION, 'readmitted-parse-screen-batch', 'isolated-parse-screen-revalidation'),
                (driver.LIBRARY_REJECTION, 'readmitted-library-batch', 'isolated-batch-library-revalidation')):
            with self.subTest(reason=reason):
                validation = {'status': 'failed', 'candidate': 'candidate', 'source_checkpoint': 'parent',
                              'reason': reason}
                r.write(folder/'validation.json', validation)
                event = {'job_id': 'one', 'attempt_id': 'attempt', 'checkpoint': 'parent', 'manifest': 'id',
                         'evidence': {'kind': evidence_kind},
                         'previous_adjudication': {'status': 'integration-rejected', 'checkpoint': None,
                                                   'evidence': {'manifest': 'id', 'validation': validation}}}
                self.queue.audit = lambda: [{'kind': event_kind, 'data': event}]
                self.assertEqual(driver._batch_observation(self.run, set(), self.queue, 'parent')['outcome'], 'rejected')
                with patch.dict(event, attempt_id='stale'), self.assertRaisesRegex(ValueError, 'disposition is missing or differs'):
                    driver._batch_observation(self.run, set(), self.queue, 'parent')

    def test_mechanical_rehabilitation_preserves_original_staging_rejection(self):
        folder = self.run/'.cache/batches/id'
        folder.mkdir(parents=True)
        job = {'job_id': 'job', 'attempt_id': 'attempt', 'status': 'integration-rejected'}
        r.write(folder/'manifest.json', {'id': 'id', 'base_commit': 'parent', 'status': 'no-proposals',
                                        'jobs': [job]})
        verdict = {'status': 'integration-rejected', 'checkpoint': None,
                   'evidence': {'manifest': 'id', 'reason': 'merge or staging rejected'}}
        self.queue.superseded = [{'id': 'job', 'attempt_id': 'attempt', 'adjudication': verdict}]
        event = {'parent_job': 'job', 'origin_attempt': 'attempt', 'checkpoint': 'parent'}
        self.queue.audit = lambda: [{'kind': 'rehabilitated-unused', 'data': event}]
        self.assertEqual(driver._batch_observation(self.run, set(), self.queue, 'parent')['outcome'], 'rejected')
        for field in ('parent_job', 'origin_attempt', 'checkpoint'):
            with self.subTest(field=field), patch.dict(event, {field: 'different'}), \
                    self.assertRaisesRegex(ValueError, 'disposition is missing or differs'):
                driver._batch_observation(self.run, set(), self.queue, 'parent')
        with patch.dict(verdict['evidence'], manifest='different'), \
                self.assertRaisesRegex(ValueError, 'disposition is missing or differs'):
            driver._batch_observation(self.run, set(), self.queue, 'parent')

    def test_mechanical_threshold_takes_only_mechanics_and_caps_at_96(self):
        config = r.read(self.run/'config.json')
        config.update(mechanical_cohorts_first=True, recovered_cohorts_first=True, recovered_union_trial=True)
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=5, ready=110)
        self.queue.ready = ([{'id': 'author-old', 'job': {'kind': 'author'}}]
                            + [{'id': f'mech-{i:03d}', 'job': {'kind': 'mechanical'}} for i in range(105)]
                            + [{'id': 'author-new', 'job': {'kind': 'author'}}])
        def selected(run, ids, *, combine_selected):
            self.assertEqual(ids, [f'mech-{i:03d}' for i in range(96)])
            self.assertTrue(combine_selected)
            self.outcome(drain=True)(run)
        with patch.object(driver, 'recovered_cohort') as cohort, \
                patch.object(driver.dispatch, '_batch', side_effect=selected) as batch:
            driver.run_driver(self.run, self.runner)
        cohort.assert_not_called()
        batch.assert_called_once()
        self.assertEqual(r.read(self.run/'batch-driver.json')['lane'], 'mechanical')

    def test_small_mechanical_lane_fills_fifo_authors(self):
        config = r.read(self.run/'config.json')
        config.update(mechanical_cohorts_first=True, recovered_union_trial=True)
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=5, ready=5)
        self.queue.ready = [{'id': name, 'job': {'kind': kind}} for name, kind in
                            [('a1', 'author'), ('m1', 'mechanical'), ('a2', 'author'),
                             ('m2', 'mechanical'), ('a3', 'author')]]
        def selected(run, ids, *, combine_selected):
            self.assertEqual(ids, ['m1', 'm2', 'a1', 'a2'])
            self.assertFalse(combine_selected)
            self.outcome(drain=True)(run)
        with patch.object(driver.dispatch, '_batch', side_effect=selected):
            driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['lane'], 'mechanical-first')

    def test_configured_mechanical_minimum_dispatches_before_general_batch_is_ready(self):
        config = r.read(self.run/'config.json')
        config.update(first_batch_size=64, batch_size=64, mechanical_cohorts_first=True,
                      mechanical_batch_minimum=2, recovered_cohorts_first=True)
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=5, ready=3)
        self.queue.ready = [{'id': name, 'job': {'kind': kind}} for name, kind in
                            [('a1', 'author'), ('m1', 'mechanical'), ('m2', 'mechanical')]]
        with patch.object(driver, 'recovered_cohort') as cohort, \
                patch.object(driver.dispatch, '_batch', side_effect=RuntimeError('batch selected')) as batch, \
                self.assertRaisesRegex(RuntimeError, 'batch selected'):
            driver.run_driver(self.run, self.runner)
        cohort.assert_not_called()
        batch.assert_called_once_with(self.run, ['m1', 'm2'], combine_selected=True)

    def test_mechanical_minimum_validates_bounds_and_type(self):
        config = r.read(self.run/'config.json')
        for value in (0, 97, True):
            with self.subTest(value=value):
                config['mechanical_batch_minimum'] = value
                r.write(self.run/'config.json', config)
                with patch.object(driver.dispatch, '_batch') as batch, \
                        self.assertRaisesRegex(ValueError, 'mechanical_batch_minimum must be between 1 and 96'):
                    driver.run_driver(self.run, self.runner)
                batch.assert_not_called()

    def test_small_mechanical_lane_preserves_recovered_cohort(self):
        config = r.read(self.run/'config.json')
        config.update(mechanical_cohorts_first=True, recovered_cohorts_first=True, fresh_union_trial=True)
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=5, ready=2)
        self.queue.ready = [{'id': 'm1', 'job': {'kind': 'mechanical'}},
                            {'id': 'a1', 'job': {'kind': 'author'}}]
        def selected(run, ids, *, combine_selected):
            self.assertEqual(ids, ['recovered'])
            self.assertFalse(combine_selected)
            self.outcome(drain=True)(run)
        cohort = {'jobs': ['recovered'], 'manifest': 'batch', 'checkpoint': 'old'}
        with patch.object(driver, 'recovered_cohort', return_value=cohort), \
                patch.object(driver.dispatch, '_batch', side_effect=selected):
            driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['lane'], 'recovered')

    def test_fresh_union_keeps_fifo_or_mechanical_priority_with_the_batch_limit(self):
        for prioritize_mechanical, expected in (
            (False, ['a1', 'm1', 'a2', 'a3']),
            (True, ['m1', 'm2', 'a1', 'a2']),
        ):
            with self.subTest(prioritize_mechanical=prioritize_mechanical):
                config = r.read(self.run/'config.json')
                config.update(fresh_union_trial=True, mechanical_cohorts_first=prioritize_mechanical,
                              recovered_cohorts_first=True)
                r.write(self.run/'config.json', config)
                self.queue.value = progress(pending=5, ready=5)
                self.queue.ready = [{'id': name, 'job': {'kind': kind}} for name, kind in
                                    [('a1', 'author'), ('m1', 'mechanical'), ('a2', 'author'),
                                     ('a3', 'author'), ('m2', 'mechanical')]]
                with patch.object(driver, 'recovered_cohort', return_value=[]), \
                        patch.object(driver.dispatch, '_batch', side_effect=RuntimeError('staging reached')) as batch, \
                        self.assertRaisesRegex(RuntimeError, 'staging reached'):
                    driver.run_driver(self.run, self.runner)
                batch.assert_called_once_with(self.run, expected, combine_selected=True)

    def test_fresh_union_requires_boolean(self):
        config = r.read(self.run/'config.json')
        config['fresh_union_trial'] = 1
        r.write(self.run/'config.json', config)
        with patch.object(driver.dispatch, '_batch') as batch, \
                self.assertRaisesRegex(ValueError, 'fresh_union_trial must be boolean'):
            driver.run_driver(self.run, self.runner)
        batch.assert_not_called()

    def test_small_mechanical_lane_waits_for_existing_trigger(self):
        config = r.read(self.run/'config.json')
        config['mechanical_cohorts_first'] = True
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=5, ready=1)
        self.queue.ready = [{'id': 'm1', 'job': {'kind': 'mechanical'}}]
        def sleep(_):
            self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'idle')
            r.write(self.run/'batch-driver-drain.json', {'reason': 'stop'})
        with patch.object(driver.time, 'sleep', side_effect=sleep), \
                patch.object(driver.dispatch, '_batch') as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_not_called()

    def test_mechanical_priority_default_off_and_invalid_flag(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver, 'mechanical_lane') as lane, \
                patch.object(driver.dispatch, '_batch', side_effect=self.outcome(drain=True)):
            driver.run_driver(self.run, self.runner)
        lane.assert_not_called()
        (self.run/'batch-driver-drain.json').unlink()
        config = r.read(self.run/'config.json')
        config['mechanical_cohorts_first'] = 'yes'
        r.write(self.run/'config.json', config)
        with patch.object(driver.dispatch, '_batch') as batch, self.assertRaisesRegex(ValueError, 'must be boolean'):
            driver.run_driver(self.run, self.runner)
        batch.assert_not_called()

    def test_recovered_cohort_is_immediate_and_dispatched_as_one_selection(self):
        config = r.read(self.run/'config.json')
        config['recovered_cohorts_first'] = True
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=1, ready=1)
        def selected(run, ids, *, combine_selected):
            self.assertEqual(ids, ['recovered'])
            self.assertTrue(combine_selected)
            self.outcome(drain=True)(run)
        with patch.object(driver, 'recovered_cohort', return_value={'jobs': ['recovered'], 'manifest': 'batch', 'checkpoint': 'parent'}) as cohort, \
                patch.object(driver.dispatch, '_batch', side_effect=selected) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once()
        cohort.assert_called_once_with(self.queue, 'parent')

    def test_old_checkpoint_cohort_stays_selected_without_combined_trial(self):
        config = r.read(self.run/'config.json')
        config['recovered_cohorts_first'] = True
        r.write(self.run/'config.json', config)
        self.queue.value = progress(pending=1, ready=1, checkpoint='parent')
        def selected(run, ids, *, combine_selected):
            self.assertEqual(ids, ['recovered'])
            self.assertFalse(combine_selected)
            self.outcome(drain=True)(run)
        with patch.object(driver, 'recovered_cohort', return_value={'jobs': ['recovered'], 'manifest': 'batch',
                                                                  'checkpoint': 'old-parent'}), \
                patch.object(driver.dispatch, '_batch', side_effect=selected) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once()

    def test_enabled_without_recovered_cohort_keeps_fifo_dispatch(self):
        config = r.read(self.run/'config.json')
        config.update(recovered_cohorts_first=True, recovered_union_trial=True)
        r.write(self.run/'config.json', config)
        self.queue.value = progress(ready=4)
        with patch.object(driver, 'recovered_cohort', return_value=[]), \
                patch.object(driver.dispatch, '_batch', side_effect=self.outcome(drain=True)) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once_with(self.run)

    def test_recovered_routing_defaults_off_and_rejects_nonboolean(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver, 'recovered_cohort') as cohort, \
                patch.object(driver.dispatch, '_batch', side_effect=self.outcome(drain=True)):
            driver.run_driver(self.run, self.runner)
        cohort.assert_not_called()
        (self.run/'batch-driver-drain.json').unlink()
        config = r.read(self.run/'config.json')
        config['recovered_cohorts_first'] = 'true'
        r.write(self.run/'config.json', config)
        with patch.object(driver.dispatch, '_batch') as batch, self.assertRaisesRegex(ValueError, 'must be boolean'):
            driver.run_driver(self.run, self.runner)
        batch.assert_not_called()

    def test_union_opt_in_alone_keeps_ordinary_fifo_routing(self):
        config = r.read(self.run/'config.json'); config['recovered_union_trial'] = True
        r.write(self.run/'config.json', config); self.queue.value = progress(ready=4)
        with patch.object(driver, 'recovered_cohort') as cohort, \
                patch.object(driver.dispatch, '_batch', side_effect=self.outcome(drain=True)) as batch:
            driver.run_driver(self.run, self.runner)
        cohort.assert_not_called(); batch.assert_called_once_with(self.run)

    def test_union_trial_rejects_every_nonboolean_before_dispatch(self):
        self.queue.value = progress(ready=4)
        for value in (0, 1, None, 'true', [], {}):
            with self.subTest(value=value):
                config = r.read(self.run/'config.json'); config['recovered_union_trial'] = value
                r.write(self.run/'config.json', config)
                with patch.object(driver.dispatch, '_batch') as batch, \
                        self.assertRaisesRegex(ValueError, 'recovered_union_trial must be boolean'):
                    driver.run_driver(self.run, self.runner)
                batch.assert_not_called()

    def test_ready_threshold_runs_once_with_owned_lease_and_observes_result(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        self.assertEqual(batch.call_count, 1)
        state = r.read(self.run/'batch-driver.json')
        self.assertEqual(state['phase'], 'first-pass-complete')
        self.assertEqual(state['last_batch']['validation_status'], 'passed')
        self.assertEqual(r._LEASES, [])

    def test_final_recovery_keeps_batch_owner_until_survivors_are_ready(self):
        self.queue.value = progress(pending=0)

        def recovery_finishes(_):
            state = r.read(self.run/'batch-driver.json')
            self.assertEqual(state['phase'], 'idle')
            self.assertEqual(state['reason'], 'automatic batch recovery is active')
            self.queue.value = progress(pending=0, ready=2)

        with patch.object(driver, 'awaiting_print_recovery', side_effect=[True, False]), \
             patch.object(driver.time, 'sleep', side_effect=recovery_finishes), \
             patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once_with(self.run)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'first-pass-complete')

    def test_readmission_during_final_check_is_observed_before_exit(self):
        self.queue.value = progress(pending=0)

        def completed_during_poll(_):
            if self.queue.value['checkpoint'] == 'parent':
                self.queue.value = progress(pending=0, ready=2)
            return False

        with patch.object(driver, 'awaiting_print_recovery', side_effect=completed_during_poll), \
             patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once_with(self.run)

    def test_controller_drain_leaves_external_batch_driver_running(self):
        self.queue.value = progress(ready=4)
        r.write(self.run/'controller-drain.json', {'reason': 'replace author runner'})
        with patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        batch.assert_called_once_with(self.run)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'first-pass-complete')

    def test_flush_once_and_no_pending_both_trigger_partial_batch(self):
        for flush in (False, True):
            with self.subTest(flush=flush), tempfile.TemporaryDirectory() as sub:
                self.run = Path(sub)
                r.write(self.run/'config.json', {'first_batch_size': 4, 'batch_size': 4,
                                               'initial_commit': 'parent', 'batch_wait_seconds': 900})
                self.queue.value = progress(pending=1 if flush else 0, ready=1)
                with patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
                    driver.run_driver(self.run, self.runner, flush_once=flush)
                self.assertEqual(batch.call_count, 1)

    def test_idle_wait_policy_and_drain_before_batch(self):
        self.queue.value = progress(pending=1, ready=1)
        def sleep(_):
            self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'idle')
            r.write(self.run/'drain.json', {'reason': 'stop'})
        with patch.object(driver.dispatch, '_batch') as batch, patch.object(driver.time, 'sleep', side_effect=sleep):
            driver.run_driver(self.run, self.runner)
        batch.assert_not_called()
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'drained')

    def test_elapsed_wait_triggers_and_rechecks_runner(self):
        self.queue.value = progress(ready=1)
        now = [0]
        def sleep(_):
            now[0] = 901
        with patch.object(driver.time, 'monotonic', side_effect=lambda: now[0]), \
                patch.object(driver.time, 'sleep', side_effect=sleep), \
                patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        self.assertEqual(batch.call_count, 1)
        self.assertEqual(self.validator.call_count, 2)

    def test_finished_work_needs_integrated_publication(self):
        self.queue.value = progress(pending=0)
        driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'needs-operator')

    def test_owned_drain_waits_for_current_batch(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver.dispatch, '_batch', side_effect=self.outcome(drain=True)) as batch:
            driver.run_driver(self.run, self.runner)
        self.assertEqual(batch.call_count, 1)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'drained')

    def test_failure_and_missing_publication_require_operator(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver.dispatch, '_batch', side_effect=self.outcome(validation='failed')):
            with self.assertRaisesRegex(ValueError, 'failed operationally'):
                driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'needs-operator')
        self.queue.value = progress(pending=0, failed=1)
        driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'needs-operator')

    def test_semantic_rejections_continue_and_preserve_checkpoint(self):
        for stage in ('no-proposals', 'provisional'):
            with self.subTest(stage=stage), tempfile.TemporaryDirectory() as sub:
                self.run = Path(sub)
                r.write(self.run/'config.json', {'first_batch_size': 1, 'batch_size': 1,
                                               'initial_commit': 'parent', 'batch_wait_seconds': 900})
                self.queue.value = progress(pending=0, ready=2)
                self.queue.rejected = []
                calls = []
                def process(run):
                    calls.append('batch')
                    if len(calls) == 2:
                        folder = self.run/'.cache/batches/good'
                        folder.mkdir(parents=True)
                        r.write(folder/'manifest.json', {'id': 'good', 'base_commit': 'parent', 'status': 'provisional',
                                'commit': 'candidate', 'jobs': [{'job_id': 'good', 'attempt_id': 'a2', 'status': 'included'}]})
                        r.write(folder/'validation.json', {'status': 'passed', 'candidate': 'candidate',
                                                          'source_checkpoint': 'parent'})
                        self.queue.value = progress(pending=0, ready=0, checkpoint='candidate')
                        r.write(self.run/'publication.json', {'status': 'integrated'})
                        return
                    folder = self.run/'.cache/batches/bad'
                    folder.mkdir(parents=True)
                    item = {'job_id': 'bad', 'attempt_id': 'a1',
                            'status': 'included' if stage == 'provisional' else 'integration-rejected'}
                    r.write(folder/'manifest.json', {'id': 'bad', 'base_commit': 'parent', 'status': stage,
                                                    'commit': 'bad-candidate', 'jobs': [item]})
                    if stage == 'provisional':
                        r.write(folder/'validation.json', {'status': 'failed', 'candidate': 'bad-candidate',
                                'source_checkpoint': 'parent', 'reason': 'full allowed-population checks failed',
                                'full_checks': {'faces': 2, 'failures': ['native readings changed or failed validation']}})
                    evidence = ({'manifest': 'bad', 'validation': r.read(folder/'validation.json')} if stage == 'provisional'
                                else {'manifest': 'bad', 'reason': 'merge or staging rejected'})
                    self.queue.rejected = [{'id': 'bad', 'attempt_id': 'a1', 'adjudication': {'evidence': evidence}}]
                    self.queue.value = progress(pending=0, ready=1)
                with patch.object(driver.dispatch, '_batch', side_effect=process):
                    driver.run_driver(self.run, self.runner)
                self.assertEqual(len(calls), 2)
                self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'first-pass-complete')

    def test_rejected_batch_cannot_advance_checkpoint_or_hide_dispositions(self):
        self.queue.value = progress(pending=0, ready=1)
        def process(run):
            folder = self.run/'.cache/batches/bad'
            folder.mkdir(parents=True)
            r.write(folder/'manifest.json', {'id': 'bad', 'base_commit': 'parent', 'status': 'no-proposals',
                    'jobs': [{'job_id': 'bad', 'attempt_id': 'a1', 'status': 'integration-rejected'}]})
            self.queue.rejected = [{'id': 'bad', 'attempt_id': 'a1',
                                    'adjudication': {'evidence': {'manifest': 'bad', 'reason': 'merge or staging rejected'}}}]
            self.queue.value = progress(pending=0, ready=0, checkpoint='wrong')
        with patch.object(driver.dispatch, '_batch', side_effect=process):
            with self.assertRaisesRegex(ValueError, 'unexpectedly advanced'):
                driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'needs-operator')

    def test_stage_failure_stops_driver(self):
        self.queue.value = progress(pending=0, ready=1)
        def process(run):
            folder = self.run/'.cache/batches/bad'
            folder.mkdir(parents=True)
            r.write(folder/'manifest.json', {'id': 'bad', 'base_commit': 'parent', 'status': 'failed',
                    'failure_scope': 'baseline-library-control',
                    'jobs': [{'job_id': 'bad', 'attempt_id': 'a1', 'status': 'integration-rejected'}]})
        with patch.object(driver.dispatch, '_batch', side_effect=process):
            with self.assertRaisesRegex(ValueError, 'staging failed operationally'):
                driver.run_driver(self.run, self.runner)
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'needs-operator')

    def test_waits_for_kernel_batch_lease_and_singleton_rejects_other_driver(self):
        failures = []
        def execute():
            try:
                driver.run_driver(self.run, self.runner, poll_seconds=0.01)
            except Exception as error:
                failures.append(error)
        with (self.run/'batch.lock').open('a') as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            worker = threading.Thread(target=execute)
            worker.start()
            deadline = time.monotonic()+3
            while not (self.run/'batch-driver.json').exists() and time.monotonic() < deadline:
                time.sleep(.01)
            self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'waiting-lock')
            with self.assertRaises(BlockingIOError):
                driver.run_driver(self.run, self.runner)
            r.write(self.run/'batch-driver-drain.json', {})
        worker.join(3)
        self.assertFalse(worker.is_alive())
        self.assertEqual(failures, [])
        self.assertEqual(r.read(self.run/'batch-driver.json')['phase'], 'drained')


if __name__ == '__main__':
    unittest.main()
