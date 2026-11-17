import json
import hashlib
import io
import tarfile
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import uuid
from unittest.mock import patch

import retire as r
import retire_tasks as tasker
import retire_queue
import retire_reuse
import retire_worker as worker


class TaskTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)
        self.source = self.base/'source'
        self.source.mkdir()
        for directory in ('grammar/syntax', 'grammar/tail', 'families', 'tests/fixtures'):
            (self.source/directory).mkdir(parents=True)
        for path in ('grammar/syntax/mined.ebnf', 'grammar/tail/mined.ebnf',
                     'grammar/syntax/actions.ebnf', 'grammar/syntax/library.ebnf',
                     'grammar/syntax/nouns.ebnf', 'grammar/lexical.ebnf'):
            (self.source/path).write_text('original\n')
        (self.source/'.gitignore').write_text('.cache/\n')
        self.git('init', '-q')
        self.git('config', 'user.email', 'tests@example.invalid')
        self.git('config', 'user.name', 'Test')
        self.git('add', '.')
        self.git('commit', '-qm', 'base')
        self.commit = self.git('rev-parse', 'HEAD')
        (self.source/'.cache').mkdir()
        (self.source/'.cache/ignored').write_text('private')
        self.campaign = self.base/'campaign'
        runner = self.campaign/'runner'
        runner.mkdir(parents=True)
        here = Path(tasker.__file__).parent
        for name in tasker.RUNNERS:
            shutil.copy2(here/name, runner/name)
        r.write(self.campaign/'runner.json', r.tree(runner))
        self.helpers = self.base/'helpers'
        self.helpers.mkdir()
        for name in tasker.dev.BINS:
            (self.helpers/name).write_text(name)
        build = {'source_sha256': 'compiled', 'binaries': {n: r.digest(self.helpers/n) for n in tasker.dev.BINS}}
        r.write(self.helpers/'build.json', build)
        self.inputs = self.base/'inputs'
        frozen = self.inputs/'frozen'
        frozen.mkdir(parents=True)
        r.write(frozen/'build.json', build)
        (frozen/'bin').mkdir()
        for name in r.HELPERS:
            shutil.copy2(self.helpers/name, frozen/'bin'/name)
        r.write(frozen/'index.json', {'0': {'name': 'A', 'record': 0, 'face': 0, 'flags': [True]*4},
                                      '1': {'name': 'B', 'record': 1, 'face': 0, 'flags': [False]*4}})
        r.jsonl(frozen/'gold.jsonl', [{'faces': [{'name': 'A', 'abilities': []}]},
                                      {'faces': [{'name': 'B', 'abilities': []}]}])
        r.write(frozen/'oracle.json', [{'cards': [{'name': 'A', 'text': 'A text'}]},
                                        {'cards': [{'name': 'B', 'text': 'B text'}]}])
        r.write(self.inputs/'manifest.json', {'format': 3, 'source': 'compiled', 'commit': self.commit,
                                              'files': r.tree(frozen), 'tooling': r.tooling()})
        self.checkpoint = {'commit': self.commit, 'source': self.source, 'inputs': self.inputs,
                           'helpers': self.helpers, 'baseline': self.base/'baseline'}
        self.job = {'id': 'job-123', 'rules': ['m_one'], 'goal': 'test',
                    'target_faces': ['A'], 'allowedwritepaths': ['grammar/syntax/actions.ebnf'],
                    'source_fragments': [{'name': 'm_one', 'sem': 'example'}]}
        self.cohort = {'names': ['A', 'B'], 'sampled': False}
        self.patch = patch.object(tasker.dev, 'source', return_value='compiled')
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        tasker._packet_data.cache_clear()
        self.tmp.cleanup()

    def test_helper_identity_rejects_manifest_or_frozen_binary_changes(self):
        tasker._identity(self.checkpoint, self.campaign, self.campaign/'runner')
        frozen = self.inputs/'frozen'
        original = (frozen/'build.json').read_bytes()
        (frozen/'build.json').write_bytes(original+b' ')
        with self.assertRaisesRegex(ValueError, 'build manifests differ'):
            tasker._identity(self.checkpoint, self.campaign, self.campaign/'runner')
        (frozen/'build.json').write_bytes(original)
        (frozen/'bin/score').write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'frozen helper checksum mismatch'):
            tasker._identity(self.checkpoint, self.campaign, self.campaign/'runner')

    def test_rejects_native_helper_corrupted_during_copy(self):
        copy2 = shutil.copy2
        def corrupt(source, destination, *args, **kwargs):
            result = copy2(source, destination, *args, **kwargs)
            destination = Path(destination)
            if destination.parent.name == 'native' and destination.name == 'probe':
                destination.write_text('corrupt copy')
            return result
        with patch.object(tasker.shutil, 'copy2', side_effect=corrupt):
            with self.assertRaisesRegex(ValueError, 'native helper changed while copying'):
                self.prepare()
        self.assertEqual((self.helpers/'probe').read_text(), 'probe')
        self.assertFalse((self.campaign/'.cache/attempts/aaaaaaaaaaaa4aaa8aaaaaaaaaaaaaaa').exists())

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.source, text=True).strip()

    def prepare(self, attempt='aaaaaaaaaaaa4aaa8aaaaaaaaaaaaaaa'):
        return tasker.prepare_task(self.campaign, {'attempt_id': attempt, 'base_commit': self.commit,
                                                   'job': self.job}, self.checkpoint, self.job, self.cohort)

    def test_versioned_runner_copies_selected_author_and_keeps_canonical_unchanged(self):
        canonical = self.campaign/'runner'
        old = r.tree(canonical)
        selected = self.campaign/'runners/v7'
        selected.parent.mkdir()
        shutil.copytree(canonical, selected)
        (selected/'retire_author_pool.py').write_text((selected/'retire_author_pool.py').read_text()+'\n# version seven\n')
        r.write(selected.with_suffix('.json'), r.tree(selected))
        claim = {'attempt_id': 'cccccccccccc4ccc8ccccccccccccccc', 'base_commit': self.commit, 'job': self.job}
        result = tasker.prepare_task(self.campaign, claim, self.checkpoint, self.job, self.cohort, runner=selected)
        support = Path(result['run'])/'support/runner/retire_author_pool.py'
        self.assertEqual(support.read_bytes(), (selected/'retire_author_pool.py').read_bytes())
        self.assertNotEqual(support.read_bytes(), (canonical/'retire_author_pool.py').read_bytes())
        self.assertEqual(r.tree(canonical), old)
        self.assertEqual(tasker.validate_runner(self.campaign), canonical)

    def test_runner_rejects_unbound_modified_and_symlinked_versions(self):
        selected = self.campaign/'runners/v7'
        selected.parent.mkdir()
        shutil.copytree(self.campaign/'runner', selected)
        with self.assertRaisesRegex(ValueError, 'missing'):
            tasker.validate_runner(self.campaign, selected)
        r.write(selected.with_suffix('.json'), r.tree(selected))
        self.assertEqual(tasker.validate_runner(self.campaign, selected), selected)
        (selected/'retire.py').write_text('changed frozen tooling')
        r.write(selected.with_suffix('.json'), r.tree(selected))
        with self.assertRaisesRegex(ValueError, 'frozen runner tooling mismatch'):
            tasker.prepare_task(self.campaign, {'attempt_id': 'dddddddddddd4ddd8ddddddddddddddd',
                                'base_commit': self.commit, 'job': self.job},
                                self.checkpoint, self.job, self.cohort, runner=selected)
        (selected/'retire_author_pool.py').write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'runner changed'):
            tasker.validate_runner(self.campaign, selected)
        with self.assertRaisesRegex(ValueError, 'direct versioned'):
            tasker.validate_runner(self.campaign, self.base/'outside')
        alias = self.campaign/'runners/link'
        alias.symlink_to(selected, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            tasker.validate_runner(self.campaign, alias)

    def test_private_attempt_and_manifest(self):
        self.checkpoint['family_case_modes'] = True
        result = self.prepare()
        run, checkout = Path(result['run']), Path(result['checkout'])
        self.assertEqual(r.verify(run/'inputs')['commit'], self.commit)
        descriptor = r.read(run/'tasks/job-123.json')
        support = r.read(run/'support.json')
        self.assertEqual(support['tasks']['job-123'], r.digest(run/'tasks/job-123.json'))
        self.assertTrue((run/'workers').is_dir())
        self.assertEqual(support['packet_sha256'], r.digest(run/'packet.json'))
        self.assertEqual(support['files'], r.tree(run/'support'))
        self.assertEqual(descriptor['rule_dispositions_required'], ['m_one'])
        self.assertEqual(descriptor['module_hints'], ['grammar/syntax/actions.ebnf'])
        self.assertIn('grammar/syntax/library.ebnf', descriptor['write_files'])
        self.assertIn('grammar/syntax/nouns.ebnf', descriptor['write_files'])
        self.assertIn('grammar/syntax/mined.ebnf', descriptor['write_files'])
        self.assertIn('grammar/tail/mined.ebnf', descriptor['write_files'])
        self.assertNotIn('grammar/lexical.ebnf', descriptor['write_files'])
        self.assertNotIn('crates/oracle-ir/src/lib.rs', descriptor['write_files'])
        self.assertEqual(descriptor['allowedwritepaths'], descriptor['write_files'])
        self.assertIn('root dispatch hooks', descriptor['goal'])
        self.assertEqual(descriptor['max_diagnostic_attempts'], 3)
        self.assertIs(descriptor['family_case_modes'], True)
        self.assertEqual(descriptor['model'], 'gpt-6.1-terra')
        self.assertEqual(descriptor['effort'], 'high')
        self.assertEqual(descriptor['cohort'], ['A', 'B'])
        packet = r.read(run/'packet.json')
        self.assertEqual([g['value']['faces'][0]['name'] for g in packet['gold_records']], ['A'])
        self.assertEqual([x['value']['cards'][0]['name'] for x in packet['oracle_records']], ['A'])
        self.assertEqual(packet['baseline'][0]['record'], 0)
        self.assertFalse((checkout/'.cache/ignored').exists())
        self.assertTrue((checkout/'.git').is_file())
        file = 'grammar/syntax/actions.ebnf'
        self.assertNotEqual(os.stat(checkout/file).st_ino, os.stat(self.source/file).st_ino)
        (checkout/file).write_text('changed')
        self.assertEqual((self.source/file).read_text(), 'original\n')
        other = self.prepare('bbbbbbbbbbbb4bbb8bbbbbbbbbbbbbbb')
        self.assertNotEqual(result['run'], other['run'])
        other_native = Path(other['run'])/'support/native'
        for name in tasker.HELPERS:
            source, first, second = self.helpers/name, run/'support/native'/name, other_native/name
            self.assertEqual(len({p.stat().st_ino for p in (source, first, second)}), 3)
            expected = r.digest(source)
            self.assertEqual((r.digest(first), r.digest(second)), (expected, expected))
            original_mode = source.stat().st_mode
            first.chmod(0o600)
            first.write_text('worker changed its private helper')
            self.assertEqual(r.digest(source), expected)
            self.assertEqual(r.digest(second), expected)
            self.assertEqual(source.stat().st_mode, original_mode)
            self.assertEqual(second.stat().st_mode, original_mode)
        self.assertEqual(support, r.read(run/'support.json'))

    def test_reversed_companions_and_duplicate_record_addresses_are_retained(self):
        frozen = self.inputs/'frozen'
        r.write(frozen/'index.json', {
            '0': {'name': 'A', 'record': 0, 'face': 0, 'text': 'first A', 'flags': [True]*4},
            '1': {'name': 'B', 'record': 0, 'face': 1, 'text': 'first B', 'flags': [True]*4},
            '2': {'name': 'A', 'record': 1, 'face': 1, 'text': 'second A', 'flags': [True]*4},
            '3': {'name': 'B', 'record': 1, 'face': 0, 'text': 'second B', 'flags': [True]*4}})
        r.jsonl(frozen/'gold.jsonl', [{'faces': [{'name': 'A'}, {'name': 'B'}]},
                                      {'faces': [{'name': 'B'}, {'name': 'A'}]}])
        r.write(frozen/'oracle.json', [
            {'cards': [{'name': 'B', 'text': 'first B'}, {'name': 'A', 'text': 'first A'}]},
            {'cards': [{'name': 'A', 'text': 'second A'}, {'name': 'B', 'text': 'second B'}]}])
        manifest = r.read(self.inputs/'manifest.json')
        manifest['files'] = r.tree(frozen)
        r.write(self.inputs/'manifest.json', manifest)
        tasker._packet_data.cache_clear()
        result = self.prepare()
        packet = r.read(Path(result['run'])/'packet.json')
        self.assertEqual([row['record'] for row in packet['gold_records']], [0, 1])
        self.assertEqual([row['record'] for row in packet['oracle_records']], [0, 1])
        self.assertEqual([row['text'] for row in packet['baseline']], ['first A', 'second A'])
        self.assertEqual(packet['gold_records'][0]['value']['faces'][0]['name'], 'A')
        self.assertEqual(packet['oracle_records'][0]['value']['cards'][0]['name'], 'B')

    def test_cohort_closure_covers_chained_shared_names_and_all_occurrences(self):
        frozen = self.inputs/'frozen'
        gold = [{'faces': [{'name': 'A'}, {'name': 'B'}]},
                {'faces': [{'name': 'B'}, {'name': 'C'}]},
                {'faces': [{'name': 'C'}]}]
        oracle = [{'cards': [{'name': 'B'}, {'name': 'A'}]},
                  {'cards': [{'name': 'C'}, {'name': 'B'}]},
                  {'cards': [{'name': 'C'}]}]
        index = {}
        for record, value in enumerate(gold):
            for face, entry in enumerate(value['faces']):
                name = entry['name']
                row = {'name': name, 'text': f'{record}:{face}', 'record': record, 'face': face,
                       'parse': {'name': name, 'address': [record, face]}, 'flags': [True]*4}
                index[str(len(index))] = row
        r.write(frozen/'index.json', index)
        r.jsonl(frozen/'gold.jsonl', gold)
        r.write(frozen/'oracle.json', oracle)
        manifest = r.read(self.inputs/'manifest.json')
        manifest['files'] = r.tree(frozen)
        r.write(self.inputs/'manifest.json', manifest)
        tasker._packet_data.cache_clear()
        self.cohort = {'names': ['A'], 'sampled': False, 'scope': 'bounded triage'}
        result = self.prepare()
        run = Path(result['run'])
        task = r.read(run/'tasks/job-123.json')
        self.assertEqual(task['target_faces'], ['A'])
        self.assertEqual(task['cohort'], ['A', 'B', 'C'])
        meta = task['cohort_detail']
        self.assertEqual(meta['closure_added_names'], ['B', 'C'])
        self.assertEqual(meta['closure_occurrence_count'], 5)
        self.assertEqual(meta['closure_record_count'], 3)
        self.assertEqual(meta, r.read(run/'packet.json')['cohort'])
        selected_dir = self.base/'slice'
        selected_dir.mkdir()
        selected = worker.subset(selected_dir, set(task['cohort']), (index, gold, oracle))
        sliced_gold = r.rows(selected_dir/'gold.jsonl')
        before = r.rows(selected_dir/'before.jsonl')
        scored_rows = [{'name': face['name'], 'address': [record, pos]}
                       for record, row in enumerate(sliced_gold) for pos, face in enumerate(row['faces'])]
        self.assertEqual(selected, {'A', 'B', 'C'})
        self.assertEqual(sliced_gold, gold)
        self.assertEqual(before, scored_rows)
        self.assertEqual([row['name'] for row in before], ['A', 'B', 'B', 'C', 'C'])
        self.assertEqual(r.read(selected_dir/'oracle.json'), oracle)
        with self.assertRaises(tasker.CohortClosureLimit) as caught:
            tasker.close_cohort(self.inputs, self.cohort, max_occurrences=4)
        self.assertEqual(caught.exception.count, 5)
        self.assertEqual(caught.exception.limit, 4)

    def test_reject_mismatched_identity_and_population(self):
        self.checkpoint['commit'] = 'wrong'
        with self.assertRaises(ValueError):
            self.prepare()
        self.checkpoint['commit'] = self.commit
        self.job['target_faces'] = ['outside']
        with self.assertRaises(ValueError):
            self.prepare()

    def test_real_queue_claim_uuid_prepares_task(self):
        queue = retire_queue.Queue(self.base/'queue.db')
        queue.add_jobs([self.job])
        queue.add_checkpoint(self.commit, {'verified': True})
        claim = queue.claim('worker-1')
        self.assertEqual(str(uuid.UUID(claim['attempt_id'])), claim['attempt_id'])
        result = tasker.prepare_task(self.campaign, claim, self.checkpoint, self.job, self.cohort)
        self.assertEqual(Path(result['run']).name, claim['attempt_id'])

    def test_verified_reuse_restores_patch_fixture_and_hook(self):
        first = self.prepare()
        prior_run, prior_checkout = Path(first['run']), Path(first['checkout'])
        task = r.read(prior_run/'tasks/job-123.json')
        (prior_checkout/'grammar/syntax/actions.ebnf').write_text('candidate\n')
        fixture = prior_checkout/task['fixture']
        fixture.parent.mkdir(parents=True, exist_ok=True)
        fixture.write_text('{"cases": []}')
        (prior_checkout/'.cache/author-hooks.json').write_text('[]')
        patch_bytes = subprocess.check_output(['git', 'diff', '--binary', '--full-index'], cwd=prior_checkout)
        names = ['grammar/syntax/actions.ebnf', task['fixture'], '.cache/author-hooks.json']
        contents = {name: (prior_checkout/name).read_bytes() for name in names}
        files = {name: {'sha256': hashlib.sha256(data).hexdigest(),
                        'mode': (prior_checkout/name).stat().st_mode & 0o777}
                 for name, data in contents.items()}
        attempt = prior_run.name
        manifest = {'format': 1, 'task': 'job-123', 'attempt': attempt, 'base_commit': self.commit,
                    'descriptor_sha256': r.digest(prior_run/'tasks/job-123.json'),
                    'patch_sha256': hashlib.sha256(patch_bytes).hexdigest(), 'files': files, 'deleted': []}
        folder = self.campaign/'.cache/archives'
        folder.mkdir()
        archive_path = folder/(attempt+'.tar.gz')
        with tarfile.open(archive_path, 'w:gz') as archive:
            for name, data in [('manifest.json', json.dumps(manifest).encode()), ('candidate.patch', patch_bytes),
                               *[('files/'+name, data) for name, data in contents.items()]]:
                entry = tarfile.TarInfo(name)
                entry.size = len(data)
                entry.mode = files[name[6:]]['mode'] if name.startswith('files/') else 0o600
                archive.addfile(entry, io.BytesIO(data))
        reuse = {'path': str(archive_path), 'sha256': r.digest(archive_path), 'attempt': attempt}
        with self.assertRaises(ValueError):
            retire_reuse.restore_candidate(self.campaign, prior_checkout, 'job-123', 'wrong-base', task['write_files'], reuse)
        with self.assertRaises(ValueError):
            retire_reuse.restore_candidate(self.campaign, prior_checkout, 'job-123', self.commit, [], reuse)
        claim = {'attempt_id': 'bbbbbbbbbbbb4bbb8bbbbbbbbbbbbbbb', 'base_commit': self.commit,
                 'job': self.job, 'reuse_archive': reuse}
        result = tasker.prepare_task(self.campaign, claim, self.checkpoint, self.job, self.cohort)
        checkout, run = Path(result['checkout']), Path(result['run'])
        self.assertEqual((checkout/'grammar/syntax/actions.ebnf').read_text(), 'candidate\n')
        self.assertEqual((checkout/task['fixture']).read_text(), '{"cases": []}')
        self.assertEqual((checkout/'.cache/author-hooks.json').read_text(), '[]')
        descriptor = r.read(run/'tasks/job-123.json')
        self.assertEqual(descriptor['reuse']['prior_attempt'], attempt)
        self.assertIn('infrastructure-tainted', descriptor['goal'])
        self.assertEqual(r.read(run/'packet.json')['reuse']['sha256'], reuse['sha256'])
        bad_claim = dict(claim, attempt_id='cccccccccccc4ccc8ccccccccccccccc',
                         reuse_archive=dict(reuse, sha256='bad'))
        with self.assertRaises(ValueError):
            tasker.prepare_task(self.campaign, bad_claim, self.checkpoint, self.job, self.cohort)

    def test_missing_modules_marked_for_discovery(self):
        self.job['allowedwritepaths'] = ['grammar/syntax/nonexistent.ebnf']
        result = self.prepare()
        task = r.read(Path(result['run'])/'tasks/job-123.json')
        self.assertTrue(task['module_discovery_required'])
        self.assertIn('grammar/syntax/actions.ebnf', task['write_files'])
        self.assertIn('grammar/syntax/nouns.ebnf', task['module_hints'])
        self.assertNotIn('grammar/syntax/nonexistent.ebnf', task['write_files'])


if __name__ == '__main__':
    unittest.main()
