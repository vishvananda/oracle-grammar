import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_batch as batch


def native(count):
    return {'reading_preservation': {'passed': True, 'missing': [], 'additions': [], 'merged_faces': 0,
                                     'old_readings': count},
            'readings': {'total': count, 'valid': count, 'canonical': count},
            'source_gold': {'valid': count}, 'canonical_gold': {'valid': count, 'idempotent': count}}


class GatedRoundTripTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)/'full-checks'
        self.directory.mkdir()
        faces = [('A', 'Alpha.'), ('B', 'Beta.'), ('A', 'Alpha.'), ('C', 'Gamma.')]
        self.index = {f'{name}{at}': {'name': name, 'text': text, 'flags': [True, True, True, True],
                                      'parse': {'name': name, 'text': text, 'status': 'equal', 'readings': 1}}
                      for at, (name, text) in enumerate(faces)}
        gold = [{'faces': [{'name': name}]} for name, _ in faces]
        oracle = [{'cards': [{'name': name}]} for name, _ in faces]
        self.data = (self.index, gold, oracle)
        self.index['B1']['flags'] = [True, False, False, False]
        # B prints neither exactly nor by default, so only its round trip can verify it.
        self.render = {'A': {'exact': True}, 'B': {'exact': False, 'default': None}, 'C': {'exact': True}}
        self.calls = []

    def execute(self, run, folder, grammar, program, arguments, threads):
        self.calls.append((folder.name, program, [str(a) for a in arguments if str(a).startswith('--')]))
        if program == 'validate-readings':
            r.write(folder/'native.json', native(len(r.rows(folder/'gold.jsonl'))))
            return
        names = [face['name'] for record in r.rows(folder/'gold.jsonl') for face in record['faces']]
        rows = []
        for name in names:
            text = next(e['text'] for e in self.index.values() if e['name'] == name)
            row = {'name': name, 'text': text, 'status': 'equal', 'readings': 1, **self.render[name]}
            if '--round-trip' in arguments:
                row['round_trip'] = [3, 1]
            rows.append(row)
        r.jsonl(folder/'rows.jsonl', rows)

    def test_reprints_only_the_faces_a_round_trip_can_still_verify(self):
        with patch.object(r, 'execute', side_effect=self.execute):
            report, reprinted = batch._gated_full_checks(Path('run'), self.directory, Path('grammar'),
                                                         ['A', 'B', 'C'], self.data, 8)
        self.assertEqual(report, {'faces': 4, 'failures': []})
        self.assertEqual(reprinted, 1)
        self.assertEqual(self.calls, [('full-checks', 'score', ['--render']),
                                      ('round-trip', 'score', ['--render', '--round-trip']),
                                      ('full-checks', 'validate-readings', ['--preserve-from'])])
        rows = r.rows(self.directory/'rows.jsonl')
        self.assertEqual([row['name'] for row in rows], ['A', 'B', 'A', 'C'])
        self.assertEqual([('round_trip' in row) for row in rows], [False, True, False, False])
        self.assertEqual([r.flags(row) for row in rows][1], [True, False, False, False])

    def test_no_reprint_when_every_row_is_exact_or_default(self):
        self.render['B'] = {'exact': False, 'default': 'Beta, again.'}
        self.index['B1']['flags'] = [True, False, True, False]
        with patch.object(r, 'execute', side_effect=self.execute):
            report, reprinted = batch._gated_full_checks(Path('run'), self.directory, Path('grammar'),
                                                         ['A', 'B', 'C'], self.data, 8)
        self.assertEqual((report['failures'], reprinted), ([], 0))
        self.assertFalse((self.directory/'round-trip').exists())

    def test_a_disagreeing_reprint_is_refused(self):
        def drifting(run, folder, grammar, program, arguments, threads):
            self.execute(run, folder, grammar, program, arguments, threads)
            if folder.name == 'round-trip' and program == 'score':
                rows = r.rows(folder/'rows.jsonl')
                rows[0]['readings'] = 2
                r.jsonl(folder/'rows.jsonl', rows)
        with patch.object(r, 'execute', side_effect=drifting), \
                self.assertRaisesRegex(ValueError, 'disagrees'):
            batch._gated_full_checks(Path('run'), self.directory, Path('grammar'), ['A', 'B', 'C'], self.data, 8)

    def test_round_trip_decides_only_unverified_equal_rows(self):
        self.assertFalse(batch.needs_round_trip({'status': 'equal', 'exact': True}))
        self.assertFalse(batch.needs_round_trip({'status': 'equal', 'exact': False, 'default': 'x'}))
        self.assertFalse(batch.needs_round_trip({'status': 'refused'}))
        self.assertTrue(batch.needs_round_trip({'status': 'equal', 'exact': False, 'default': None}))
        self.assertTrue(batch.needs_round_trip({'status': 'equal', 'printing': [
            {'reading': 0, 'exact': True}, {'reading': 1, 'exact': False}]}))
        with self.assertRaisesRegex(ValueError, 'all or gated'):
            batch._round_trip_mode({'full_check_round_trip': True})
        self.assertEqual(batch._round_trip_mode({}), 'all')


if __name__ == '__main__':
    unittest.main()
