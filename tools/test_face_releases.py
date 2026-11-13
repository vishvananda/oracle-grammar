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

    def test_export_pipeline_checks_original_source_and_exact_producer(self):
        from contextlib import contextmanager
        import shutil
        source = self.root/'source.gz'
        with gzip.open(source, 'wt') as stream:
            stream.write(json.dumps({'oracle_id': 'synthetic', 'name': 'Fixture'})+'\n')
        spec = {**self.spec, 'source': {**self.source, 'sha256': releases.digest(source)}}
        exporter = {**self.meta, 'grammar': {**self.meta['grammar'], 'commit': 'e'*40}}
        producer_root = self.root/'released'; producer_root.mkdir()
        binary = self.root/'native-export'
        binary.write_text('#!/usr/bin/env python3\nimport json,sys\n'
                          'row={"identity":{"record_index":0,"face_index":0},'
                          '"schema_version":190,"format_version":1,"status":"refused","readings":[]}\n'
                          'open(sys.argv[2],"w").write(json.dumps(row)+"\\n")\n')
        binary.chmod(0o755)
        @contextmanager
        def exact_producer(tag, root):
            self.assertEqual(tag, 'grammar-v0.1.0')
            self.assertEqual(root, self.root)
            yield producer_root, self.meta
        def build(root):
            self.assertEqual(root, producer_root)
            return self.root
        def metadata(root):
            return exporter if root == self.root else self.meta
        def fetch(url, path, checksum):
            self.assertEqual(checksum, releases.digest(source))
            shutil.copyfile(source, path)
        with patch.object(faces, 'specification', return_value=spec), \
             patch.object(faces, 'producer', side_effect=exact_producer), \
             patch.object(releases, 'metadata', side_effect=metadata), \
             patch.object(releases, 'build', side_effect=build), \
             patch.object(releases, 'fetch', side_effect=fetch):
            out = self.root/'release'
            faces.export(out, grammar_tag='grammar-v0.1.0', root=self.root)
            manifest = releases.verify(out)
            self.assertEqual(manifest['grammar']['commit'], 'a'*40)
            self.assertEqual(manifest['exporter']['commit'], 'e'*40)
            self.assertEqual(manifest['counts']['counts']['refused'], 1)
            binary.write_text('#!/usr/bin/env python3\nimport sys\nopen(sys.argv[2],"w").close()\n')
            with self.assertRaisesRegex(ValueError, 'missing input face'):
                faces.export(self.root/'incomplete', grammar_tag='grammar-v0.1.0', root=self.root)
            self.assertFalse((self.root/'incomplete').exists())

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
