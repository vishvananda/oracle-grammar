"""Cohort inputs stay hash-pinned and comparisons stay by occurrence."""
import json
from pathlib import Path
import shutil
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
        self.model = self.root/'model.json'
        self.model.write_text('{"schema_version":190}')
        patcher = patch.object(corpus, 'MODEL', self.model)
        patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(corpus, 'migrate_reference', side_effect=shutil.copyfile)
        self.migration = patcher.start()
        self.addCleanup(patcher.stop)
        patcher = patch.object(corpus, 'migration_identity', return_value='frozen-test-migration')
        self.migration_identity = patcher.start()
        self.addCleanup(patcher.stop)
        revision = corpus.pin.reference()['revision']
        self.updates = self.root/'updates.json'
        self.updates.write_text(json.dumps({'version': 1, 'reference': revision, 'updates': []}))
        patcher = patch.object(corpus.updates, 'UPDATES', self.updates)
        patcher.start()
        self.addCleanup(patcher.stop)
        (self.root/'allowed.json').write_text(json.dumps({'cards': ['A', 'B', 'Split']}))
        (self.root/'oracle.json').write_text(json.dumps([
            {'cards': [{'name': 'A', 'text': 'Draw a card.'}]},
            {'cards': [{'name': 'Sealed', 'text': 'Hidden.'}]},
            {'cards': [{'name': 'Split', 'text': 'One.'}, {'name': 'Sealed', 'text': 'Two.'}]},
            {'cards': [{'name': 'B', 'text': 'Discard a card.'}]}]))
        corpus.write_rows(self.root/'reference.jsonl', [
            {'faces': [{'name': 'A', 'abilities': [{'kind': 'Draw', 'args': 1}]}]},
            {'faces': [{'name': 'Sealed'}]},
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

    def test_prepare_applies_recorded_export_updates(self):
        revision = corpus.pin.reference()['revision']
        def record(face, before, after):
            return {'version': 1, 'reference': revision, 'updates': [{
                'id': 'U0001', 'kind': 'correction', 'summary': 'Draw two.',
                'evidence': 'The card says two.', 'changes': [
                    {'face': face, 'path': [0, 'args'], 'from': before, 'to': after}]}]}
        self.updates.write_text(json.dumps(record('A', 1, 2)))
        prepared = corpus.prepare(corpus.inputs())
        self.assertEqual(corpus.prepare(corpus.inputs()), prepared)
        gold = corpus.rows(prepared/'gold.jsonl')
        self.assertEqual(gold[0]['faces'][0]['abilities'], [{'kind': 'Draw', 'args': 2}])
        self.assertEqual(json.loads((prepared/'updates.json').read_text())['applied'],
                         {'U0001': ['A']})
        self.assertEqual(corpus.updated(prepared, [row('A', 'equal')]), {'U0001': {'equal': 1}})
        for face, before, after, message in (('A', 3, 4, 'stale'), ('A', 0, 1, 'adopted upstream'),
                                             ('Sealed', 1, 2, 'missing from')):
            self.updates.write_text(json.dumps(record(face, before, after)))
            with self.assertRaisesRegex(ValueError, message):
                corpus.prepare(corpus.inputs())

    def test_migration_precedes_amendments_and_receipts_include_the_model(self):
        revision = corpus.pin.reference()['revision']
        self.updates.write_text(json.dumps({'version': 1, 'reference': revision, 'updates': [{
            'id': 'U0001', 'kind': 'correction', 'summary': 'Current-epoch amendment.',
            'evidence': 'Apply after the owned migration.', 'changes': [
                {'face': 'A', 'path': [0, 'args'], 'from': 5, 'to': 9}]}]}))

        def migrate(source, destination):
            values = corpus.rows(source)
            self.assertNotIn('Sealed', source.read_text())
            values[0]['faces'][0]['abilities'][0]['args'] = 5
            for record in values:
                record['schema_version'] = 191
                for face in record['faces']:
                    face['schema_version'] = 191
            corpus.write_rows(destination, values)

        self.migration.side_effect = migrate
        first = corpus.prepare(corpus.inputs())
        self.assertEqual(corpus.rows(first/'gold.jsonl')[0]['faces'][0]['abilities'][0]['args'], 9)
        self.assertEqual(corpus.rows(first/'gold.jsonl')[0]['schema_version'], 191)
        self.assertEqual(corpus.prepare(corpus.inputs()), first)
        self.assertEqual(self.migration.call_count, 1)
        self.model.write_text('{"schema_version":191}')
        self.assertNotEqual(corpus.prepare(corpus.inputs()), first)
        self.assertEqual(self.migration.call_count, 2)
        self.migration_identity.return_value = 'updated-test-migration'
        third = corpus.prepare(corpus.inputs())
        self.assertNotEqual(third, first)
        self.assertEqual(self.migration.call_count, 3)

    def test_failed_migration_cannot_publish_a_prepared_cache(self):
        self.migration.side_effect = ValueError('invalid frozen input')
        with self.assertRaisesRegex(ValueError, 'invalid frozen input'):
            corpus.prepare(corpus.inputs())
        self.assertEqual(list((corpus.WORK/'prepared').iterdir()), [])

    def test_model_change_during_migration_cannot_publish_a_prepared_cache(self):
        def changed_model(source, destination):
            shutil.copyfile(source, destination)
            self.model.write_text('{"schema_version":191}')

        self.migration.side_effect = changed_model
        with self.assertRaisesRegex(ValueError, 'changed during preparation'):
            corpus.prepare(corpus.inputs())
        self.assertEqual(list((corpus.WORK/'prepared').iterdir()), [])

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

    def test_gaps_credit_a_stuck_point_with_the_rules_only_its_faces_need(self):
        stuck = [row('A', 'refused', 0, syntax={'frontier': '. If you ^ Do,'}),
                 row('B', 'refused', 0, syntax={'frontier': '. if you ^ do,'}),
                 row('C', 'refused', 0, syntax={'frontier': 'X is the ^ amount'}),
                 row('D', 'equal')]
        needed = {'m_a': ['A'], 'm_b': ['B'], 'm_shared': ['A', 'C'], 'm_d': ['D']}
        first, second = corpus.gap_ranking(stuck, needed)
        self.assertEqual(first, {'key': '. if you ^ do,', 'faces': ['A', 'B'],
                                 'retirable': ['m_a', 'm_b']})
        self.assertEqual(second['retirable'], [])


if __name__ == '__main__':
    unittest.main()
