"""Cohort inputs stay hash-pinned and comparisons stay by occurrence."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import corpus


def row(name, status, readings=1, **extra):
    return {'name': name, 'text': f'{name} text', 'status': status, 'readings': readings, **extra}


class CorpusTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        for target, value in (('WORK', self.root/'work'), ('MANIFEST', self.root/'work/inputs.json'),
                              ('ISSUES', self.root/'issues.md')):
            patcher = patch.object(corpus, target, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        (self.root/'work').mkdir()
        (self.root/'allowed.json').write_text(json.dumps({'cards': ['A', 'B', 'Split']}))
        (self.root/'oracle.json').write_text(json.dumps([
            {'cards': [{'name': 'A', 'text': 'Draw a card.'}]},
            {'cards': [{'name': 'Sealed', 'text': 'Hidden.'}]},
            {'cards': [{'name': 'Split', 'text': 'One.'}, {'name': 'Sealed', 'text': 'Two.'}]},
            {'cards': [{'name': 'B', 'text': 'Discard a card.'}]}]))
        corpus.write_rows(self.root/'reference.jsonl', [
            {'faces': [{'name': 'A'}]}, {'faces': [{'name': 'Sealed'}]},
            {'faces': [{'name': 'Split'}, {'name': 'Sealed'}]}, {'faces': [{'name': 'B'}]}])
        corpus.write_rows(self.root/'baseline.jsonl', [row('A', 'equal'), row('Split', 'refused'),
                                                        row('B', 'different')])
        manifest = {name: {'path': str(self.root/file), 'sha256': corpus.pin.digest(self.root/file)}
                    for name, file in (('allowed', 'allowed.json'), ('oracle', 'oracle.json'),
                                       ('reference', 'reference.jsonl'),
                                       ('baseline', 'baseline.jsonl'))}
        corpus.write(corpus.MANIFEST, manifest)

    def test_prepare_drops_sealed_faces_keeps_order_and_verifies_reuse(self):
        prepared = corpus.prepare(corpus.inputs())
        oracle = json.loads((prepared/'oracle.json').read_text())
        self.assertEqual([[f['name'] for f in r['cards']] for r in oracle], [['A'], ['Split'], ['B']])
        self.assertNotIn('Sealed', (prepared/'gold.jsonl').read_text())
        self.assertEqual(corpus.prepare(corpus.inputs()), prepared)
        (prepared/'gold.jsonl').write_text('{}\n')
        with self.assertRaisesRegex(ValueError, 'prepared inputs changed'):
            corpus.prepare(corpus.inputs())

    def test_inputs_reject_a_changed_pinned_file(self):
        (self.root/'baseline.jsonl').write_text('')
        with self.assertRaisesRegex(ValueError, 'baseline checksum mismatch'):
            corpus.inputs()

    def test_compare_fails_lost_matches_parses_and_limits(self):
        before = [row('A', 'equal'), row('B', 'different'), row('C', 'refused')]
        report = corpus.compare(before, [row('A', 'equal'), row('B', 'equal'),
                                         row('C', 'different', 2)])
        self.assertTrue(report['passed'])
        self.assertEqual(report['gained_matches'], ['B'])
        self.assertEqual(report['new_parses'], ['C'])
        self.assertEqual(report['reading_counts'], ['C'])
        lost = corpus.compare(before, [row('A', 'refused', 0, why='Limit(work)'),
                                       row('B', 'different'), row('C', 'refused')])
        self.assertFalse(lost['passed'])
        self.assertEqual((lost['lost_matches'], lost['lost_parses'], lost['limits']),
                         (['A'], ['A'], ['A']))
        documented = corpus.compare(before, [row('A', 'different'), row('B', 'different'),
                                             row('C', 'refused')], frozenset({'A'}))
        self.assertTrue(documented['passed'])
        self.assertEqual(documented['documented_losses'], ['A'])
        refused = corpus.compare(before, [row('A', 'refused', 0), row('B', 'different'),
                                          row('C', 'refused')], frozenset({'A'}))
        self.assertFalse(refused['passed'])
        with self.assertRaisesRegex(ValueError, 'order or population'):
            corpus.compare(before, before[::-1])

    def test_overlay_replaces_duplicate_names_by_occurrence(self):
        baseline = [row('A', 'refused'), row('B', 'equal'), row('A', 'different')]
        current = [row('A', 'equal'), row('A', 'equal', 2)]
        result = corpus.overlay(baseline, current, {'A'})
        self.assertEqual([r['status'] for r in result], ['equal', 'equal', 'equal'])
        self.assertEqual(result[2]['readings'], 2)
        with self.assertRaisesRegex(ValueError, 'outnumber'):
            corpus.overlay(baseline, current+[row('A', 'equal')], {'A'})

    def test_slices_stay_inside_the_allowed_population(self):
        args = type('Args', (), {'name': 'controls', 'all': False, 'unmatched': True,
                                 'cards': None, 'regex': 'draw'})
        with patch('builtins.print'):
            corpus.make_slice(args)
        cards = json.loads(corpus.slice_path('controls').read_text())['cards']
        self.assertEqual(cards, ['A', 'B', 'Split'])
        args.unmatched, args.regex, args.cards = False, None, ['Sealed']
        with self.assertRaisesRegex(ValueError, 'outside the allowed population'):
            corpus.make_slice(args)
        with self.assertRaisesRegex(ValueError, 'lowercase'):
            corpus.slice_path('../escape')

    def test_queue_signature_names_the_smallest_differing_node(self):
        gold = [{'kind': 'Effect', 'args': {'source': {'kind': 'Source'}}}]
        far = [{'kind': 'Spell', 'args': {}}]
        near = [{'kind': 'Effect', 'args': {'source': {'kind': 'EventSubject'}}}]
        key, _ = corpus.signature(row('A', 'different', 2, gold=gold, all=[far, near]))
        self.assertEqual(key, 'Source -> EventSubject at args/source')
        key, _ = corpus.signature(row('B', 'refused', 0, why='NoParse',
                                      syntax={'frontier': 'Draw a ^ Card in hand'}))
        self.assertEqual(key, 'syntax before: card in')


if __name__ == '__main__':
    unittest.main()
