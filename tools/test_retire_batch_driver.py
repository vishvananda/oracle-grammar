import fcntl
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

import retire as r
import retire_batch_driver as driver


class QueueStub:
    def __init__(self, progress):
        self.value = progress
        self.rejected = []
    def get_jobs(self, status):
        return self.rejected if status == 'integration-rejected' else []
    def progress(self):
        return self.value


def progress(pending=1, running=0, ready=0, failed=0, checkpoint='parent'):
    counts = {'pending': pending, 'running': running, 'provisional-pass': ready, 'failed': failed}
    return {'checkpoint': checkpoint, 'counts': counts, 'job_counts': dict(counts)}


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

    def test_ready_threshold_runs_once_with_owned_lease_and_observes_result(self):
        self.queue.value = progress(ready=4)
        with patch.object(driver.dispatch, '_batch', side_effect=self.outcome()) as batch:
            driver.run_driver(self.run, self.runner)
        self.assertEqual(batch.call_count, 1)
        state = r.read(self.run/'batch-driver.json')
        self.assertEqual(state['phase'], 'first-pass-complete')
        self.assertEqual(state['last_batch']['validation_status'], 'passed')
        self.assertEqual(r._LEASES, [])

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
                                'source_checkpoint': 'parent', 'reason': 'full allowed-population checks failed'})
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
