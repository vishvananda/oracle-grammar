import json
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_parse_screen as screen


class ParseScreenTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.run = self.root/'run'
        (self.run/'frozen').mkdir(parents=True)
        self.out = self.root/'screen'
        self.out.mkdir()
        r.write(self.run/'manifest.json', {'population': 2})
        r.write(self.run/'frozen/allowed.json', {'cards': ['A', 'B']})
        r.write(self.out/'slice.json', {'cards': ['A', 'B']})
        self.old = [{'name': 'A', 'text': 'a', 'status': 'equal', 'readings': 1},
                    {'name': 'B', 'text': 'b', 'status': 'equal', 'readings': 1}]
        self.data = ({str(i): {'name': row['name'], 'text': row['text'], 'parse': row}
                      for i, row in enumerate(self.old)}, [], [])
        r.jsonl(self.out/'before.jsonl', self.old)
        r.jsonl(self.out/'rows.jsonl', self.old)
        r.jsonl(self.out/'commands.jsonl', [{'program': 'score', 'returncode': 0},
                                          {'program': 'validate-readings', 'returncode': 0}])
        (self.out/'native-failures.jsonl').write_text('')
        self.native = {'compiled_schema': 192, 'gold_source_schemas': {'192': 2},
                       'source_gold': {'faces': 2, 'valid': 2, 'invalid_reasons': {}},
                       'canonical_gold': {'faces': 2, 'valid': 2, 'idempotent': 2, 'invalid_reasons': {}},
                       'recognized_faces': 2, 'faces_with_any_valid_reading': 2,
                       'faces_with_all_readings_valid': 2,
                       'readings': {'total': 2, 'valid': 2, 'canonical': 2, 'invalid_reasons': {}},
                       'reading_preservation': {'passed': True, 'occurrences': 2, 'old_readings': 2,
                                                'explicit_readings': 0, 'implicit_equal_singletons': 2,
                                                'native_normalized_distinct_readings': 2,
                                                'missing': [], 'additions': [], 'merged_faces': 0}}
        self.write_native()

    def write_native(self):
        r.write(self.out/'native.json', self.native)

    def test_clean_screen_is_only_preliminary(self):
        result = screen.assess(self.run, self.out, self.data, 0)
        self.assertEqual(result['status'], 'passed')
        self.assertIn('full print validation remains required', result['scope'])

    def test_cli_zero_with_addition_is_rejected(self):
        self.native['reading_preservation']['additions'] = [{'name': 'A', 'occurrence': 0, 'previous_status': 'equal',
                                                              'added_current_indices': [0], 'retained_current_indices': []}]
        self.write_native()
        self.assertTrue(screen.assess(self.run, self.out, self.data, 0)['failures'])

    def test_cli_one_requires_complete_native_failure_receipt(self):
        r.jsonl(self.out/'commands.jsonl', [{'program': 'score', 'returncode': 0},
                                          {'program': 'validate-readings', 'returncode': 1}])
        with self.assertRaises(ValueError):
            screen.assess(self.run, self.out, self.data, 1)
        self.native['reading_preservation'].update(passed=False, missing=[{'name': 'A', 'reading': 0}])
        self.write_native()
        self.assertEqual(screen.assess(self.run, self.out, self.data, 1)['status'], 'failed')

    def test_malformed_or_incomplete_evidence_fails_closed(self):
        for mutation in ('population', 'slice', 'baseline', 'score', 'schema', 'native', 'receipt', 'missing'):
            with self.subTest(mutation=mutation):
                self.setUp()
                if mutation == 'population': r.write(self.run/'manifest.json', {'population': 3})
                if mutation == 'slice': r.write(self.out/'slice.json', {'cards': ['A']})
                if mutation == 'baseline': r.jsonl(self.out/'before.jsonl', self.old[:1])
                if mutation == 'score': r.jsonl(self.out/'rows.jsonl', self.old[:1])
                if mutation == 'schema': self.native['gold_source_schemas'] = {'192': 1}; self.write_native()
                if mutation == 'native': self.native['readings']['valid'] = 1; self.write_native()
                if mutation == 'receipt': r.jsonl(self.out/'commands.jsonl', [{'program': 'score', 'returncode': 1}])
                if mutation == 'missing': (self.out/'native-failures.jsonl').unlink()
                with self.assertRaises((ValueError, KeyError)):
                    screen.assess(self.run, self.out, self.data, 0)

    def test_score_and_unexpected_native_errors_are_operational(self):
        for program in ('score', 'validate-readings'):
            with self.subTest(program=program), tempfile.TemporaryDirectory() as temporary:
                target = Path(temporary)/'out'
                def execute(run, directory, grammar, name, args, threads):
                    if name == program:
                        raise subprocess.CalledProcessError(2, [name])
                with patch.object(screen.w, 'subset'), patch.object(screen.r, 'execute', side_effect=execute):
                    with self.assertRaises(subprocess.CalledProcessError):
                        screen.run(self.run, target, self.root, self.data, 6)

    def test_row_difference_rejects_without_print_or_ir_comparison(self):
        rows = [dict(row, ir={'unrelated': 'value'}) for row in self.old]
        r.jsonl(self.out/'rows.jsonl', rows)
        self.assertEqual(screen.assess(self.run, self.out, self.data, 0)['status'], 'passed')
        rows[0]['text'] = 'changed'
        r.jsonl(self.out/'rows.jsonl', rows)
        self.assertEqual(screen.assess(self.run, self.out, self.data, 0)['status'], 'failed')
