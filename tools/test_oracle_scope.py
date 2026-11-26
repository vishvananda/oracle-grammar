import gzip
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import oracle_scope


class ScopeTests(unittest.TestCase):
    def test_json_comments_and_trailing_commas_keep_quoted_names(self):
        with TemporaryDirectory() as tmp:
            path = Path(tmp) / 'ignore.json5'
            path.write_text('['
                            '// heading\n'
                            '"Fast // Furious", /* note */ "A \\"quote\\"",\n'
                            ']')
            self.assertEqual(oracle_scope.read_ignores(path), {'Fast // Furious', 'A "quote"'})

    def test_exclusions_use_source_metadata_and_exact_outer_names(self):
        normal = {'name': 'Red Herring', 'set': 'mkm', 'layout': 'normal',
                  'type_line': 'Artifact Creature — Fish'}
        names = {'Sketchy Red Herring', 'Side one'}
        cases = [
            (normal, (None, None)),
            ({**normal, 'set': 'cmb2'}, ('upstream_ignore_name', 'Sketchy Red Herring')),
            ({**normal, 'layout': 'art_series'}, ('excluded_layout', None)),
            ({**normal, 'type_line': 'Dungeon'}, ('supplemental_dungeon', None)),
            ({**normal, 'type_line': 'Card'}, ('non_game_card', None)),
            ({**normal, 'layout': 'planar'}, (None, None)),
            ({**normal, 'name': 'Side one // Side two', 'layout': 'split'}, (None, None)),
        ]
        for card, expected in cases:
            with self.subTest(card=card):
                self.assertEqual(oracle_scope.exclusion(card, names), expected)

    def test_generation_checks_inputs_and_partitions_oracle_ids(self):
        with TemporaryDirectory() as tmp:
            source = Path(tmp) / 'oracle.jsonl.gz'
            ignores = Path(tmp) / 'ignore.json5'
            ignores.write_text('["Named exception"]')
            card = {'oracle_id': 'one', 'name': 'Example', 'layout': 'normal',
                    'type_line': 'Artifact', 'set': 'x'}
            with gzip.open(source, 'wt') as stream:
                for row in [card, {**card, 'oracle_id': 'two', 'name': 'Named exception'}]:
                    stream.write(json.dumps(row) + '\n')
            with patch.object(oracle_scope, 'IGNORE_SHA256', oracle_scope.sha256(ignores)):
                with self.assertRaisesRegex(ValueError, 'Oracle source SHA-256 mismatch'):
                    oracle_scope.classify(source, ignores, 'example', 'bad')
                manifest = oracle_scope.classify(source, ignores, 'example', oracle_scope.sha256(source))
            self.assertEqual(manifest['counts']['source'], 2)
            self.assertEqual(manifest['population']['oracle_ids'], ['one'])
            self.assertEqual([row['oracle_id'] for row in manifest['ignored']], ['two'])


if __name__ == '__main__':
    unittest.main()
