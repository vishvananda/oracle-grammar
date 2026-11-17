"""Mechanical diagnostics retain complete bounded shared-record cohorts."""
from collections import defaultdict
from pathlib import Path
import contextlib
import io
import json
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_cohort as cohort
import retire_dispatch as dispatch


def index_for(records):
    result = defaultdict(list)
    ordinal = 0
    for record, value in enumerate(records):
        for face, item in enumerate(value['faces']):
            result[item['name']].append((ordinal, {'name': item['name'], 'record': record, 'face': face}))
            ordinal += 1
    return result


class PartitionTests(unittest.TestCase):
    def test_partition_covers_targets_duplicate_names_and_companions(self):
        records = [{'faces': [{'name': f'n{i:03d}'}]} for i in range(501)]
        records[0]['faces'].append({'name': 'companion'})
        records.append({'faces': [{'name': 'n000'}, {'name': 'other'}]})
        targets = [f'n{i:03d}' for i in range(501)]
        chunks = cohort.mechanical_chunks(index_for(records), records, targets)
        self.assertEqual(len(chunks), 2)
        self.assertEqual(set().union(*(set(x['base_names']) for x in chunks)), set(targets))
        self.assertTrue(all(len(x['base_names']) <= 500 and x['occurrences'] <= 1024 for x in chunks))
        self.assertEqual(sum(x['occurrences'] for x in chunks), 504)
        self.assertIn('companion', chunks[0]['names'])
        self.assertIn('other', chunks[0]['names'])
        self.assertEqual(len({n for x in chunks for n in x['names']}), sum(len(x['names']) for x in chunks))

    def test_oversized_component_and_chunk_limit_fail_closed(self):
        giant = [{'faces': [{'name': f'n{i}'} for i in range(1025)]}]
        with self.assertRaises(cohort.CohortClosureLimit):
            cohort.mechanical_chunks(index_for(giant), giant, ['n0'])
        connected = [{'faces': [{'name': f'n{i}'} for i in range(501)]}]
        with self.assertRaisesRegex(ValueError, 'base-name'):
            cohort.mechanical_chunks(index_for(connected), connected, [f'n{i}' for i in range(501)])
        records = [{'faces': [{'name': f'n{i}'}]} for i in range(4001)]
        with self.assertRaisesRegex(ValueError, 'chunk count'):
            cohort.mechanical_chunks(index_for(records), records, [f'n{i}' for i in range(4001)])
        broad = [{'faces': [{'name': f'component{group}-face{i}'} for i in range(600)]}
                 for group in range(9)]
        with self.assertRaisesRegex(ValueError, 'chunk count'):
            cohort.mechanical_chunks(index_for(broad), broad, [f'component{group}-face0' for group in range(9)])


class PreparationTests(unittest.TestCase):
    def test_normal_detail_is_unchanged_and_does_not_chunk(self):
        names = ['a', 'b']
        with patch.object(dispatch, 'close_cohort'), patch.object(dispatch, 'mechanical_chunks') as chunk:
            detail, targets, plan = dispatch.mechanical_detail(Path('/input'), names)
        self.assertEqual(detail, {'names': names, 'sampled': False, 'scope': 'audited-unused literal triage',
                                  'full_batch_audit_required': True})
        self.assertEqual(targets, names)
        self.assertIsNone(plan)
        chunk.assert_not_called()

    def test_oversized_base_or_closure_receives_bounded_plan(self):
        names = [f'n{i}' for i in range(501)]
        plan = [{'base_names': names[:500], 'names': names[:500], 'occurrences': 500, 'records': 500},
                {'base_names': names[500:], 'names': names[500:], 'occurrences': 1, 'records': 1}]
        with patch.object(dispatch, 'close_cohort') as close, \
                patch.object(dispatch, 'mechanical_chunks', return_value=plan) as chunk:
            detail, targets, result = dispatch.mechanical_detail(Path('/input'), names)
        close.assert_not_called()
        chunk.assert_called_once_with(Path('/input'), names)
        self.assertEqual((detail['names'], targets, result), (names[:500], names[:500], plan))
        with patch.object(dispatch, 'close_cohort', side_effect=cohort.CohortClosureLimit(1025, 1024)), \
                patch.object(dispatch, 'mechanical_chunks', return_value=plan) as chunk:
            dispatch.mechanical_detail(Path('/input'), ['a', 'b'])
        chunk.assert_called_once_with(Path('/input'), ['a', 'b'])


class WorkerTests(unittest.TestCase):
    def run_worker(self, mode):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); run = root/'run'; (run/'inputs/frozen/grammar').mkdir(parents=True)
            (run/'inputs/frozen/grammar/test.ebnf').write_text('grammar')
            (run/'inputs/scan').mkdir()
            (run/'tasks').mkdir()
            r.write(run/'tasks/job.json', {})
            r.write(run/'support.json', {})
            r.write(run/'inputs/scan/rules.json', {'unused': ['m_rule']})
            names = [f'name{i:03d}' for i in range(501)]
            chunks = [{'base_names': names[:500], 'names': names[:500], 'occurrences': 500, 'records': 500},
                      {'base_names': names[500:], 'names': names[500:], 'occurrences': 1, 'records': 1}]
            task = {'rules': ['m_rule'], 'target_faces': chunks[0]['base_names'], 'cohort': chunks[0]['names'],
                    'mechanical_chunks': chunks, 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            if mode == 'tamper': task['mechanical_chunks'] = chunks[:1]
            expected = chunks
            calls = []
            def checks(inputs, folder, grammar, actual, data, threads):
                calls.append(list(actual))
                if mode == 'exception' and len(calls) == 1: raise ValueError('native failed')
                failures = []
                if mode == 'print-failure' and len(calls) == 2: failures = ['print gate lost']
                if mode == 'native-failure' and len(calls) == 2: failures = ['native readings changed']
                return {'faces': len(actual), 'failures': failures}
            with patch.object(dispatch.author, 'verify', return_value=(task, root, {'commit': 'base'})), \
                    patch.object(dispatch.w, 'payload', return_value=({}, [], [])), \
                    patch.object(dispatch.w, 'cohort', return_value=names), \
                    patch.object(dispatch, 'mechanical_chunks', return_value=expected) as recompute, \
                    patch.object(dispatch.mined, 'remove', return_value=(['m_rule'], {})), \
                    patch.object(dispatch.w, 'checks', side_effect=checks), \
                    patch.object(dispatch.r, 'verify'), contextlib.redirect_stdout(io.StringIO()):
                dispatch.mechanical(run, 'job')
            report = r.read(next((run/'evaluations/job').glob('*/report.json')))
            recompute.assert_called_once_with(run/'inputs', names)
            return report, calls

    def test_all_chunks_are_checked_before_success(self):
        report, calls = self.run_worker('pass')
        self.assertEqual([len(x) for x in calls], [500, 1])
        self.assertTrue(report['passed'])
        self.assertTrue(report['coverage_complete'])
        self.assertEqual(report['faces'], 501)
        self.assertEqual(report['chunks_unrun'], 0)

    def test_print_failure_never_becomes_success(self):
        report, calls = self.run_worker('print-failure')
        self.assertEqual(len(calls), 2)
        self.assertFalse(report['passed'])
        self.assertEqual(report['failures'], ['print gate lost'])
        self.assertTrue(report['coverage_complete'])

    def test_native_preservation_failure_never_becomes_success(self):
        report, calls = self.run_worker('native-failure')
        self.assertEqual(len(calls), 2)
        self.assertFalse(report['passed'])
        self.assertEqual(report['failures'], ['native readings changed'])
        self.assertTrue(report['coverage_complete'])

    def test_tampered_plan_and_partial_failure_are_explicit(self):
        report, calls = self.run_worker('tamper')
        self.assertFalse(report['passed'])
        self.assertFalse(report['coverage_complete'])
        self.assertEqual(calls, [])
        report, calls = self.run_worker('exception')
        self.assertFalse(report['passed'])
        self.assertFalse(report['coverage_complete'])
        self.assertEqual(report['chunks_started'], 1)
        self.assertEqual(report['chunks_completed'], 0)
        self.assertEqual(report['chunks_unrun'], 1)
        self.assertEqual(report['chunks_incomplete'], 2)


if __name__ == '__main__': unittest.main()
