"""Release hashes, input scope, outcome accounting and deterministic assets."""
import gzip
import json
from pathlib import Path
import tempfile
import tarfile
import io
import unittest
from unittest.mock import patch

import releases


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.meta = {'grammar': {'version': '0.1.0', 'commit': 'a'*40},
                     'model': {'schema_version': 190, 'packages': {}}}
        self.spec = {'provider': 'scryfall', 'snapshot_id': 'sha256-test', 'sha256': 'a'*64,
                     'population': {'kind': 'all', 'policy': 'Synthetic fixtures only'}}

    def row(self, index, status='unique'):
        row = {'identity': {'record_index': index, 'face_index': 0}, 'schema_version': 190,
               'format_version': 1, 'status': status, 'readings': []}
        if status in ('unique', 'ambiguous'):
            n = 1 if status == 'unique' else 2
            row.update(semantic_readings=n, readings=[{'card': {'synthetic': i}} for i in range(n)])
        return row

    def bundle(self, name, rows):
        path = self.root/name
        releases.bundle(path, rows, self.meta, self.spec, 1)
        return path

    def test_accounting_and_ambiguity_are_preserved(self):
        rows = [self.row(i, s) for i, s in enumerate(('unique', 'ambiguous', 'refused', 'limit'))]
        out = self.bundle('out', rows)
        manifest = releases.verify(out)
        self.assertEqual(manifest['counts']['input_faces'], 4)
        self.assertEqual(manifest['counts']['counts'], dict.fromkeys(('unique', 'ambiguous', 'refused', 'limit'), 1))
        with gzip.open(out/'outcomes.jsonl.gz', 'rt') as f:
            self.assertEqual([json.loads(line) for line in f], rows)
        with gzip.open(out/'faces.jsonl.gz', 'rt') as f:
            self.assertEqual([json.loads(line) for line in f], rows[:1])
        self.assertEqual(manifest['counts']['gates']['print_reparse'], 'not_run')

    def test_reordering_and_compression_are_reproducible(self):
        rows = [self.row(i) for i in range(3)]
        one, two = self.bundle('one', rows), self.bundle('two', list(reversed(rows)))
        self.assertEqual({p.name: p.read_bytes() for p in one.iterdir()},
                         {p.name: p.read_bytes() for p in two.iterdir()})

    def test_rejects_partial_success_schema_drift_and_duplicates(self):
        row = self.row(0); row['status'] = 'limit'
        for bad in ([row], [self.row(0), self.row(0)],
                    [{**self.row(0), 'schema_version': 191}],
                    [{**self.row(0), 'semantic_readings': 2}],
                    [{**self.row(0), 'status': 'ambiguous'}]):
            with self.assertRaises(ValueError):
                self.bundle('bad', bad)
            self.assertFalse((self.root/'bad').exists())

    def test_checksum_and_inventory_tampering_is_rejected(self):
        out = self.bundle('out', [self.row(0)])
        path = out/'faces.jsonl.gz'; original = path.read_bytes()
        path.write_bytes(original+b'x')
        with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
            releases.verify(out)
        path.write_bytes(original)
        (out/'extra').write_text('not listed')
        with self.assertRaisesRegex(ValueError, 'unlisted'):
            releases.verify(out)

    def test_input_manifest_requires_pinned_source_and_explicit_population(self):
        source = self.root/'oracle.json'; source.write_text('[]')
        spec = self.root/'source.json'
        value = {'format_version': 1, **self.spec, 'sha256': releases.digest(source)}
        spec.write_text(json.dumps(value))
        self.assertEqual(releases.source_spec(spec, source), value)
        for change in ({'sha256': 'b'*64}, {'population': {}}, {'snapshot_id': '../escape'},
                       {'population': {'kind': 'oracle_ids', 'policy': 'allowed', 'oracle_ids': ['x', 'x']}}):
            spec.write_text(json.dumps({**value, **change}))
            with self.assertRaises(ValueError):
                releases.source_spec(spec, source)

    def test_runtime_overrides_cannot_leak_into_releases(self):
        with patch.dict('os.environ', {'ORACLE_GRAMMAR_DIR': 'alternate'}):
            with self.assertRaisesRegex(ValueError, 'override'):
                releases.environment()

    def test_syntax_source_archive_is_deterministic(self):
        grammar = self.root/'grammar'; grammar.mkdir()
        (grammar/'one.ebnf').write_text('rule ::= "word"')
        files = list(grammar.iterdir())
        releases.archive(self.root/'one.tar.gz', files, self.root)
        releases.archive(self.root/'two.tar.gz', files, self.root)
        self.assertEqual((self.root/'one.tar.gz').read_bytes(), (self.root/'two.tar.gz').read_bytes())

    def test_face_tag_pins_the_source_specification(self):
        tag = 'faces-oracle-sha256-test-s190-g0.1.0-r1'
        directory = self.root/'releases/faces'; directory.mkdir(parents=True)
        path = directory/(tag+'.json')
        spec = {'format_version': 1, 'grammar_tag': 'grammar-v0.1.0', 'revision': 1,
                'source': {'format_version': 1, **self.spec, 'url': 'https://example.invalid/oracle.json'}}
        path.write_text(json.dumps(spec))
        from face_releases import specification
        self.assertEqual(specification(tag=tag, root=self.root, meta=self.meta), spec)
        for change in ({'grammar_tag': 'grammar-v0.2.0'}, {'revision': 2}):
            path.write_text(json.dumps({**spec, **change}))
            with self.assertRaisesRegex(ValueError, 'disagree'):
                specification(tag=tag, root=self.root, meta=self.meta)
        with self.assertRaises(ValueError):
            specification(tag='../escape', root=self.root, meta=self.meta)

    def test_release_checkout_must_remain_immutable_during_build(self):
        with patch.object(releases, 'metadata', return_value=self.meta):
            releases.unchanged(self.meta)
            with self.assertRaisesRegex(ValueError, 'checkout changed'):
                releases.unchanged({**self.meta, 'grammar': {'commit': 'b'*40}})

    def crate(self, name, *, source=b'pub struct Card;', commit=b'old', lock=b'old', manifest=b'name = "model"', normalized=None):
        path = self.root/name
        with tarfile.open(path, 'w:gz') as archive:
            for filename, data in {'src/lib.rs': source, '.cargo_vcs_info.json': commit,
                                   'Cargo.lock': lock, 'Cargo.toml': normalized or manifest+b'\n# Cargo-generated normalization',
                                   'Cargo.toml.orig': manifest}.items():
                info = tarfile.TarInfo('model-0.1.0/'+filename); info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        return path

    def test_model_reuse_ignores_regenerated_provenance_but_preserves_sources(self):
        published = self.crate('published.crate')
        current = self.crate('current.crate', commit=b'new grammar commit', lock=b'new workspace lock')
        releases.compare_crate_sources(current, published)
        self.assertNotEqual(releases.digest(current), releases.digest(published))
        for current in (self.crate('changed-source.crate', source=b'pub struct DifferentCard;'),
                        self.crate('changed-dependencies.crate', manifest=b'name = "model"\nserde = "2"'),
                        self.crate('changed-inherited-dependencies.crate', normalized=b'name = "model"\nserde = "2"')):
            with self.assertRaisesRegex(ValueError, 'bump its package version'):
                releases.compare_crate_sources(current, published)

    def test_reused_models_are_not_replaced_by_temporary_registry_versions(self):
        published = self.crate('published.crate')
        target = self.root/'target'; (target/'package').mkdir(parents=True)
        candidate = self.crate('candidate.crate', commit=b'new grammar commit')
        import shutil
        shutil.copyfile(candidate, target/'package/oracle-ir-0.1.0.crate')
        reused = self.root/'reused'; reused.mkdir()
        versions = dict.fromkeys(releases.PACKAGES, '0.1.0')
        def download(url, path, checksum):
            self.assertEqual(checksum, releases.digest(published))
            shutil.copyfile(published, path)
        with patch.object(releases, 'run', return_value=json.dumps({'target_directory': str(target)})), \
             patch.object(releases, 'registry_checksum', side_effect=lambda p, v: releases.digest(published) if p == 'oracle-ir' else None), \
             patch.object(releases, 'fetch', side_effect=download), \
             patch.object(releases.subprocess, 'run') as command:
            archives = releases.package_archives({'model': {'packages': versions}}, reused)
        self.assertEqual(releases.digest(archives['oracle-ir']), releases.digest(published))
        selected = command.call_args_list[-1].args[0]
        self.assertEqual(selected, ['cargo', 'package', '--locked', '-p', 'oracle-ir-contract', '-p', 'oracle-ir-grammar'])


if __name__ == '__main__':
    unittest.main()
