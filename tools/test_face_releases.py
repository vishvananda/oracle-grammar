"""Automatic code/data release linkage and exact immutable producer selection."""
import gzip
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import face_releases as faces
import releases


class FaceReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.meta = {'grammar': {'version': '0.1.0', 'commit': 'a'*40},
                     'model': {'schema_version': 190, 'packages': {}}}
        self.source = {'format_version': 1, 'provider': 'scryfall', 'snapshot_id': '20261112',
                       'sha256': 'b'*64, 'url': 'https://example.invalid/source.jsonl.gz',
                       'encoding': 'jsonl-gzip', 'population': {'kind': 'all', 'policy': 'Synthetic fixtures only'}}
        self.spec = {'format_version': 1, 'revision': 1, 'grammar_tag': 'grammar-v0.1.0', 'source': self.source}

    def test_automatic_and_independent_tags_share_identity(self):
        tag = 'faces-oracle-20261112-s190-g0.1.0-r1'
        directory = self.root/'releases/faces'; directory.mkdir(parents=True)
        (directory.parent/'oracle-source.json').write_text(json.dumps(self.spec))
        (directory/(tag+'.json')).write_text(json.dumps(self.spec))
        for selector in ({'tag': tag}, {'grammar_tag': 'grammar-v0.1.0'}):
            spec = faces.specification(root=self.root, meta=self.meta, **selector)
            self.assertEqual(faces.tag_for(spec, self.meta), tag)
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            faces.specification(tag=tag, grammar_tag='grammar-v0.1.0', root=self.root)
        with self.assertRaisesRegex(ValueError, 'exactly one'):
            faces.specification(root=self.root)
        with self.assertRaises(FileNotFoundError):
            faces.specification(grammar_tag='grammar-v0.1.0', root=self.root/'missing')
        updated = {**self.meta, 'grammar': {**self.meta['grammar'], 'version': '0.2.0'}}
        spec = faces.specification(grammar_tag='grammar-v0.2.0', root=self.root, meta=updated)
        self.assertEqual(faces.tag_for(spec, updated), tag.replace('g0.1.0', 'g0.2.0'))

    def test_pins_and_population_are_required(self):
        for change in ({'sha256': 'invalid'}, {'url': 'http://example.invalid/source'},
                       {'encoding': 'guess'}, {'population': {}}, {'format_version': 2}):
            with self.assertRaises(ValueError):
                faces.tag_for({**self.spec, 'source': {**self.source, **change}}, self.meta)
        for revision in (True, 0, '1'):
            with self.assertRaises(ValueError):
                faces.tag_for({**self.spec, 'revision': revision}, self.meta)

    def test_original_compressed_bytes_and_subset_are_verified(self):
        path = self.root/'source.gz'
        values = [{'oracle_id': 'one', 'oracle_text': 'Original words.'}, {'oracle_id': 'two'}]
        with gzip.open(path, 'wt') as stream:
            for value in values:
                stream.write(json.dumps(value)+'\n')
        source = {**self.source, 'sha256': releases.digest(path)}
        self.assertEqual(faces.records(path, source), values)
        source['population'] = {'kind': 'oracle_ids', 'policy': 'synthetic', 'oracle_ids': ['two']}
        self.assertEqual(faces.records(path, source), values[1:])
        source['population']['oracle_ids'] = ['missing']
        with self.assertRaisesRegex(ValueError, 'missing'):
            faces.records(path, source)
        source['sha256'] = '0'*64
        with self.assertRaisesRegex(ValueError, 'checksum'):
            faces.records(path, source)

    def test_producer_uses_tag_not_exporter_head_and_cleans_up_on_failure(self):
        def git(*args):
            return releases.run('git', *args, cwd=self.root)
        git('init', '-q')
        git('config', 'user.name', 'Fixture'); git('config', 'user.email', 'fixture@example.invalid')
        (self.root/'meaning').write_text('released')
        git('add', 'meaning'); git('commit', '-qm', 'released'); git('tag', 'grammar-v0.1.0')
        (self.root/'meaning').write_text('new unpublished meaning')
        git('commit', '-qam', 'new meaning')
        with patch.object(releases, 'metadata', return_value=self.meta):
            with self.assertRaisesRegex(RuntimeError, 'deliberate failure'):
                with faces.producer('grammar-v0.1.0', self.root) as (checkout, _):
                    self.assertEqual((checkout/'meaning').read_text(), 'released')
                    raise RuntimeError('deliberate failure')
        self.assertEqual(git('worktree', 'list', '--porcelain').count('worktree '), 1)
        for tag in ('HEAD', 'grammar-v0.1.0;echo unsafe', 'grammar-v0.9.0'):
            with self.assertRaises((ValueError, subprocess.CalledProcessError)):
                with faces.producer(tag, self.root):
                    self.fail('invalid/missing code tag accepted')

    def test_source_asset_is_archived_without_rewriting(self):
        source = self.root/'original.gz'; source.write_bytes(b'original compressed fixture')
        spec = {**self.source, 'sha256': releases.digest(source)}
        row = {'identity': {'record_index': 0, 'face_index': 0}, 'format_version': 1,
               'schema_version': 190, 'status': 'refused', 'readings': []}
        out = self.root/'bundle'
        releases.bundle(out, [row], self.meta, spec, 1, source=source)
        manifest = releases.verify(out)
        self.assertEqual((out/'oracle-source.jsonl.gz').read_bytes(), source.read_bytes())
        self.assertEqual(manifest['source_oracle']['sha256'], releases.digest(source))
        self.assertEqual(manifest['source_oracle']['encoding'], 'jsonl-gzip')


if __name__ == '__main__':
    unittest.main()
