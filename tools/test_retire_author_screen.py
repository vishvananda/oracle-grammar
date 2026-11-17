"""A small parse screen rejects early but never proves a candidate correct."""
import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_author as author
import retire_cohort as cohort


class ScreenTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.directory = self.root/'report'
        self.directory.mkdir()
        self.records = [{'faces': [{'name': 'A'}, {'name': 'X'}]},
                        {'faces': [{'name': 'X'}, {'name': 'B'}]},
                        {'faces': [{'name': 'A'}, {'name': 'C'}]}]
        self.records += [{'faces': [{'name': f'S{i}'}]} for i in range(125)]
        self.records += [{'faces': [{'name': 'OUTSIDE'}]}]
        self.index, self.oracle = {}, []
        for record, gold in enumerate(self.records):
            self.oracle.append({'cards': [{'name': face['name'], 'text': face['name']} for face in gold['faces']]})
            for face, value in enumerate(gold['faces']):
                name = value['name']
                self.index[str(len(self.index))] = {'name': name, 'text': name, 'record': record, 'face': face,
                                                  'parse': {'name': name, 'text': name, 'status': 'equal',
                                                            'readings': 1, 'all': ['old']}, 'flags': [True]*4}
        self.data = self.index, self.records, self.oracle
        self.task = {'cohort': ['A', 'X', 'B', 'C', *(f'S{i}' for i in range(125))], 'target_faces': ['A']}

    def execute(self, run, folder, grammar, program, args, threads):
        self.assertEqual(program, 'score')
        self.assertEqual(threads, 2)
        self.assertFalse(any(str(arg).startswith('--') for arg in args))
        rows = r.rows(folder/'before.jsonl')
        for row in rows:
            row['all'] = ['new raw representation is irrelevant']
        r.jsonl(folder/'rows.jsonl', rows)

    def screen(self):
        with patch.object(author.r, 'execute', side_effect=self.execute):
            return author.early_parse_screen(self.root, self.directory, self.root/'grammar', self.task, self.data)

    def test_closed_primary_preserves_duplicate_occurrences_and_ignores_raw_ir(self):
        result = self.screen()
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['primary_occurrences'], 6)
        self.assertEqual(result['full_occurrences'], 131)
        self.assertEqual(result['failures'], [])
        before = r.rows(self.directory/'parse-screen/before.jsonl')
        self.assertEqual([row['name'] for row in before], ['A', 'X', 'X', 'B', 'A', 'C'])

    def test_order_and_primary_outside_full_are_rejected(self):
        def misordered(run, folder, grammar, program, args, threads):
            rows = r.rows(folder/'before.jsonl')
            rows[0], rows[1] = rows[1], rows[0]
            r.jsonl(folder/'rows.jsonl', rows)
        with patch.object(author.r, 'execute', side_effect=misordered):
            result = author.early_parse_screen(self.root, self.directory, self.root/'grammar', self.task, self.data)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['failures'][0]['ordinal'], 0)
        self.assertEqual((result['failures'][0]['record'], result['failures'][0]['face_index']), (0, 0))
        self.task['target_faces'] = ['OUTSIDE']
        with self.assertRaisesRegex(ValueError, 'escaped full'):
            self.screen()

    def test_parse_status_and_reading_count_regressions_are_rejected(self):
        def bad_rows(run, folder, grammar, program, args, threads):
            rows = r.rows(folder/'before.jsonl')
            rows[0]['status'] = 'different'
            rows[2]['readings'] = 2
            r.jsonl(folder/'rows.jsonl', rows)
        with patch.object(author.r, 'execute', side_effect=bad_rows):
            result = author.early_parse_screen(self.root, self.directory, self.root/'grammar', self.task, self.data)
        self.assertEqual([f['ordinal'] for f in result['failures']], [0, 2])

    def test_no_screen_at_boundary_or_without_targets(self):
        self.task['target_faces'] = []
        self.assertIsNone(self.screen())
        self.task['target_faces'] = ['A']
        self.task['cohort'] = self.task['cohort'][:-3]
        self.assertIsNone(self.screen())  # 128 occurrences

    def test_closed_cohort_can_exceed_500_names_after_base_expansion(self):
        # A appears in two records, and X connects to a third: one initial
        # target grows to four distinct names through the shared closure.
        self.records.pop()  # remove OUTSIDE, then extend to 501 closed names
        self.oracle.pop()
        self.index.pop(str(len(self.index)-1))
        for number in range(125, 497):
            name = f'S{number}'
            record = len(self.records)
            self.records.append({'faces': [{'name': name}]})
            self.oracle.append({'cards': [{'name': name, 'text': name}]})
            self.index[str(len(self.index))] = {'name': name, 'text': name, 'record': record, 'face': 0,
                                              'parse': {'name': name, 'text': name, 'status': 'equal',
                                                        'readings': 1}, 'flags': [True]*4}
        by_name = {}
        for ordinal, row in self.index.items():
            by_name.setdefault(row['name'], []).append((int(ordinal), row))
        base = ['A', *(f'S{i}' for i in range(497))]
        full = cohort.close_cohort(by_name, self.records, {'names': base})
        self.assertEqual(len(full['names']), 501)
        self.task['cohort'] = full['names']
        result = self.screen()
        self.assertEqual(result['status'], 'passed')
        self.assertEqual(result['full_occurrences'], 503)
        with self.assertRaisesRegex(ValueError, 'base cohort exceeds'):
            cohort.close_cohort(by_name, self.records, {'names': full['names']})


class ScreenWorkflowTests(unittest.TestCase):
    def test_failed_screen_skips_full_and_pass_runs_every_existing_check(self):
        for screen_status in ('failed', 'passed'):
            with self.subTest(screen_status=screen_status), tempfile.TemporaryDirectory() as tmp:
                run, root = Path(tmp)/'run', Path(tmp)/'worker'
                for folder in (run/'tasks', root/'grammar/syntax', root/'families', root/'tests/fixtures'):
                    folder.mkdir(parents=True)
                (root/'grammar/syntax/example.ebnf').write_text('example ::= "example"\n')
                r.write(run/'tasks/task.json', {'task': 'test'})
                r.write(run/'support.json', {'files': {}})
                r.write(root/'families/case.json', {'cases': [{'expected': []}, {'reject': True}, {}, {}]})
                r.write(root/'tests/fixtures/edits.json', [])
                task = {'rules': ['m_seed'], 'fixture': 'families/case.json',
                        'edits': 'tests/fixtures/edits.json', 'cohort': ['Example'], 'target_faces': ['Example']}
                verification = (task, root, {'commit': 'base'})
                failures = [{'face': 'Example', 'ordinal': 0, 'gate': 'parse/reading-count/text'}]
                screen = {'status': screen_status, 'failures': failures if screen_status == 'failed' else []}
                with patch.object(author, 'verify', return_value=verification), \
                     patch.object(author, 'seeded', side_effect=[{'m_seed'}, set()]), \
                     patch.object(author, 'check_seeded_files'), \
                     patch.object(author, 'apply_hooks', return_value=[]), \
                     patch.object(author, 'helper', return_value=0), \
                     patch.object(author.w, 'payload', return_value='data'), \
                     patch.object(author, 'early_parse_screen', return_value=screen), \
                     patch.object(author.w, 'checks', return_value={'faces': 3, 'failures': []}) as checks, \
                     patch.object(author, 'edited_checks', return_value=[{'face': 'Example'}]) as edited, \
                     contextlib.redirect_stdout(io.StringIO()):
                    report = author._check(run, 'task')
                self.assertEqual(report['passed'], screen_status == 'passed')
                self.assertEqual(checks.call_count, int(screen_status == 'passed'))
                self.assertEqual(edited.call_count, int(screen_status == 'passed'))
                if screen_status == 'failed':
                    self.assertEqual(report['failures'], failures)
                    self.assertIn('full cohort print gates', report['checks_skipped']['checks'])


if __name__ == '__main__':
    unittest.main()
