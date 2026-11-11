"""Provenance checks must reject drift and leave supplied artifacts unchanged."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import pin


class PinTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ('pins/model.json', 'adapters/mtgish/reference.json', 'Cargo.toml', 'Cargo.lock'):
            path = self.root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(pin.ROOT/name, path)

    def edit(self, name, mutate):
        path = self.root/name
        value = json.loads(path.read_text())
        mutate(value)
        path.write_text(json.dumps(value))

    def test_checked_in_model_and_reference_are_consistent(self):
        pin.check_model(self.root)
        pin.reference(self.root)

    def test_model_revision_and_lock_cannot_drift(self):
        self.edit('pins/model.json', lambda p: p.update(revision='a'*40))
        with self.assertRaisesRegex(ValueError, 'dependency differs'):
            pin.check_model(self.root)

    def test_rejects_local_model_override(self):
        path = self.root/'Cargo.toml'
        path.write_text(path.read_text()+'\n[patch."https://github.com/vishvananda/mtgish.git"]\nmtgish-ast = { path = "elsewhere" }\n')
        with self.assertRaisesRegex(ValueError, 'without overrides'):
            pin.check_model(self.root)

    def test_rejects_lock_identity_drift(self):
        path = self.root/'Cargo.lock'
        path.write_text(path.read_text().replace('#7228b84b2b9398976246b595098f7f5e5fb4fc88', '#'+('b'*40)))
        with self.assertRaisesRegex(ValueError, 'Cargo.lock differs'):
            pin.check_model(self.root)

    def test_checksum_verification_never_modifies_supplied_data(self):
        path = self.root/'supplied.jsonl'
        original = b'not the pinned artifact\n'
        path.write_bytes(original)
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            pin.verify(path, root=self.root)
        self.assertEqual(path.read_bytes(), original)
        self.edit('adapters/mtgish/reference.json', lambda p: p['normalized'].update(sha256=pin.digest(path)))
        self.assertEqual(pin.verify(path, root=self.root), path)

    def test_reference_is_independent_of_model_revision(self):
        self.edit('adapters/mtgish/reference.json', lambda p: p.update(revision='c'*40))
        pin.reference(self.root)
        pin.check_model(self.root)

    def test_normalized_fetch_requires_published_url(self):
        with self.assertRaisesRegex(ValueError, 'no published download URL'):
            pin.fetch('normalized', self.root)
        self.assertFalse((self.root/'.cache').exists())

    def test_rejects_nonimmutable_reference(self):
        self.edit('adapters/mtgish/reference.json', lambda p: p.update(revision='main'))
        with self.assertRaisesRegex(ValueError, 'immutable'):
            pin.reference(self.root)


if __name__ == '__main__':
    unittest.main()
