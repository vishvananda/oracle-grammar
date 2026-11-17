"""The operator only applies rehearsed, checkpoint-bound timeout splits."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
from retire_queue import Queue
import retire_timeout_operator as operator
import test_retire_timeout_recovery as fixtures


class TimeoutOperatorTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.TimeoutRecoveryTests('runTest')
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root
        r.write(self.root/'config.json', {'expected_rules': self.fixture.rules})
        self.plan = self.root/'plan.json'
        self.receipt = self.root/'receipt.json'

    def make_plan(self):
        result = operator.dryplan(self.root, self.plan, ['family'])
        self.assertEqual(result['jobs'][0]['status'], 'rehearsed')
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'needs-review': 6})
        return result

    def test_plan_and_apply_preserve_result_and_coverage(self):
        original = self.fixture.queue.get_attempt(self.fixture.attempt)
        plan = self.make_plan()
        result = operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['completed'][0]['children'], plan['jobs'][0]['children'])
        self.assertIsNone(result['inflight'])
        self.assertEqual(self.fixture.queue.get_attempt(self.fixture.attempt), original)
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'pending': 6})
        with self.assertRaises(ValueError):
            operator.apply(self.plan, r.digest(self.plan), self.root/'another.json', self.root)

    def test_child_can_be_claimed_before_post_commit_verification(self):
        self.make_plan()
        original = Queue.rehabilitate_timeout
        def split_then_claim(queue, *args):
            result = original(queue, *args)
            self.assertIsNotNone(queue.claim('immediate-worker'))
            return result
        with patch.object(Queue, 'rehabilitate_timeout', split_then_claim):
            result = operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(self.fixture.queue.coverage()['counts']['running'], 3)

    def test_child_can_be_reclassified_before_post_commit_verification(self):
        self.make_plan()
        original = Queue.rehabilitate_timeout
        def split_then_reclassify(queue, *args):
            result = original(queue, *args)
            child = result['children'][0]
            first = next(row for row in queue.get_jobs('pending') if row['id'] == child)['job']
            rules = first['rules']
            queue.split_pending(child, [{'id': 'reclassified-one', 'rules': rules[:1]},
                                        {'id': 'reclassified-rest', 'rules': rules[1:]}], {'reason': 'test'})
            return result
        with patch.object(Queue, 'rehabilitate_timeout', split_then_reclassify):
            result = operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'pending': 6})

    def test_recovered_or_non_timeout_parent_stays_parked(self):
        self.fixture.split()
        plan = operator.dryplan(self.root, self.plan, ['family'])
        self.assertEqual(plan['jobs'][0]['status'], 'parked')
        with self.assertRaisesRegex(ValueError, 'no rehearsed'):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.assertFalse(self.receipt.exists())

    def test_nonempty_final_and_inventory_mismatch_refuse(self):
        (self.fixture.run/'workers/family/final.txt').write_text('response')
        plan = operator.dryplan(self.root, self.plan, ['family'])
        self.assertEqual(plan['jobs'][0]['status'], 'parked')
        r.write(self.root/'config.json', {'expected_rules': ['wrong']})
        with self.assertRaisesRegex(ValueError, 'inventory'):
            operator.dryplan(self.root, self.root/'other.json')

    def test_stale_checkpoint_refuses_before_receipt(self):
        self.make_plan()
        self.fixture.queue.add_checkpoint('later', {'inputs': 'unused'}, expected_parent='new')
        with self.assertRaisesRegex(ValueError, 'checkpoint'):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.assertFalse(self.receipt.exists())

    def test_plan_and_code_tampering_refuse(self):
        self.make_plan()
        original = r.digest(self.plan)
        with self.assertRaisesRegex(ValueError, 'plan hash'):
            operator.apply(self.plan, '0'*64, self.receipt, self.root)
        with patch.object(operator, 'verifier_hashes', return_value={'changed': 'digest'}):
            with self.assertRaisesRegex(ValueError, 'verifier identity'):
                operator.apply(self.plan, original, self.receipt, self.root)
        plan = r.read(self.plan)
        plan['script_sha256'] = '0'*64
        r.write(self.plan, plan)
        with self.assertRaisesRegex(ValueError, 'script'):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        self.plan.write_text(self.plan.read_text()+' ')
        with self.assertRaisesRegex(ValueError, 'plan hash'):
            operator.apply(self.plan, original, self.receipt, self.root)

    def test_artifact_tampering_reports_inflight_without_acceptance(self):
        self.make_plan()
        (self.fixture.run/'workers/family/events.jsonl').write_text('{}\n')
        with self.assertRaises(ValueError):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        receipt = r.read(self.receipt)
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual(receipt['completed'], [])
        self.assertEqual(receipt['inflight'], 'family')
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'needs-review': 6})

    def test_post_commit_receipt_write_failure_retains_result_and_stops(self):
        self.make_plan()
        write = operator.update_receipt
        attempts = []
        def transient(path, value):
            attempts.append((value['status'], len(value['completed'])))
            if value['status'] == 'applying' and value['completed']:
                raise OSError('injected receipt write failure')
            return write(path, value)
        original = Queue.rehabilitate_timeout
        with patch.object(operator, 'update_receipt', side_effect=transient), \
                patch.object(Queue, 'rehabilitate_timeout', autospec=True, side_effect=original) as split, \
                self.assertRaisesRegex(OSError, 'injected receipt'):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        split.assert_called_once()
        receipt = r.read(self.receipt)
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual([item['job_id'] for item in receipt['completed']], ['family'])
        self.assertIsNone(receipt['inflight'])
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'pending': 6})
        self.assertEqual(attempts[-1], ('failed', 1))

    def test_partial_progress_is_recorded_and_stops(self):
        plan = self.make_plan()
        second = dict(plan['jobs'][0], job_id='missing')
        plan['jobs'].append(second)
        r.write(self.plan, plan)
        with self.assertRaisesRegex(ValueError, 'planned attempt'):
            operator.apply(self.plan, r.digest(self.plan), self.receipt, self.root)
        receipt = r.read(self.receipt)
        self.assertEqual(receipt['status'], 'failed')
        self.assertEqual([item['job_id'] for item in receipt['completed']], ['family'])
        self.assertIsNone(receipt['inflight'])
        self.assertEqual(self.fixture.queue.coverage()['counts'], {'pending': 6})


if __name__ == '__main__': unittest.main()
