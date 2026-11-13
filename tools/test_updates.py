"""Export updates are validated records, written against one reference revision."""
import json
from pathlib import Path
import tempfile
import unittest

import updates

REVISION = 'a'*40


def record(**change):
    change = {'face': 'A', 'path': [0, 'args'], 'from': 1, 'to': 2, **change}
    return {'version': 1, 'reference': REVISION, 'updates': [{
        'id': 'U0001', 'kind': 'normalization', 'summary': 'One form.',
        'evidence': 'Every other record uses it.', 'changes': [change]}]}


class UpdateTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.path = Path(directory.name)/'updates.json'

    def load(self, data, revision=REVISION):
        self.path.write_text(json.dumps(data))
        return updates.load(self.path, revision)

    def test_the_recorded_updates_are_valid_against_the_pinned_reference(self):
        data = updates.load()
        self.assertTrue(all(update['changes'] for update in data['updates']))

    def test_updates_belong_to_one_reference_revision(self):
        self.assertEqual(self.load(record())['reference'], REVISION)
        with self.assertRaisesRegex(ValueError, 'review them again'):
            self.load(record(), 'b'*40)

    def test_records_name_their_kind_evidence_and_a_real_change(self):
        data = record()
        data['updates'][0]['kind'] = 'alias'
        with self.assertRaisesRegex(ValueError, 'kind is one of'):
            self.load(data)
        data = record()
        data['updates'][0]['evidence'] = ' '
        with self.assertRaisesRegex(ValueError, 'evidence are required'):
            self.load(data)
        with self.assertRaisesRegex(ValueError, 'changes nothing'):
            self.load(record(to=1))
        with self.assertRaisesRegex(ValueError, 'keys and indexes'):
            self.load(record(path=[0, -1]))
        data = record()
        data['updates'].append(dict(data['updates'][0]))
        with self.assertRaisesRegex(ValueError, 'unique'):
            self.load(data)

    def test_a_change_applies_only_where_the_recorded_value_still_is(self):
        changes = updates.pending(self.load(record()))['A']
        face = {'name': 'A', 'abilities': [{'kind': 'Draw', 'args': 1}]}
        self.assertEqual(updates.apply(face, changes), ['U0001'])
        self.assertEqual(face['abilities'][0]['args'], 2)
        with self.assertRaisesRegex(ValueError, 'adopted upstream'):
            updates.apply(face, changes)
        face['abilities'][0]['args'] = 3
        with self.assertRaisesRegex(ValueError, 'stale'):
            updates.apply(face, changes)
        with self.assertRaisesRegex(ValueError, 'no value'):
            updates.apply({'name': 'A', 'abilities': []}, changes)


if __name__ == '__main__':
    unittest.main()
