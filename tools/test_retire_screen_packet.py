"""Compact screen packets preserve complete occurrence and integrity contracts."""
import hashlib
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import retire as r
import retire_author as author
import retire_cohort as cohort
import retire_screen_packet as packet
import retire_tasks as tasker
import test_retire_author_screen as screen_tests
import test_retire_tasks as task_tests


class PacketTests(unittest.TestCase):
    setUp = screen_tests.ScreenTests.setUp

    def make_packet(self):
        by_name = {}
        for ordinal, row in self.index.items():
            by_name.setdefault(row['name'], []).append((int(ordinal), row))
        oracle_index = {}
        for ordinal, record in enumerate(self.oracle):
            key = tuple(sorted(face['name'] for face in record['cards']))
            oracle_index.setdefault(key, []).append((ordinal, record))
        full = cohort.close_cohort(by_name, self.records, {'names': self.task['cohort']}, already_closed=True)
        job = {'id': 'job', 'target_faces': self.task['target_faces']}
        value = packet.build(by_name, self.records, oracle_index, job, full, 'manifest-hash')
        self.assertIsNotNone(value)
        self.task.update(id='job', cohort_detail=full, screen_packet_sha256=hashlib.sha256(value).hexdigest())
        (self.root/'screen-packet.json').write_bytes(value)
        return {'screen_packet_sha256': self.task['screen_packet_sha256']}

    def test_old_and_compact_screens_have_exact_duplicate_occurrence_projection(self):
        # Preserve nested source object order as well as duplicate Oracle order.
        self.records[0]['faces'][0]['z_value'] = 1
        self.records[0]['faces'][0]['a_value'] = 2
        self.oracle[0]['cards'][0]['z_value'] = 1
        self.oracle[0]['cards'][0]['a_value'] = 2
        support = self.make_packet()
        compact = packet.load(self.root, self.task, 'manifest-hash', support)
        first, second = self.directory/'old', self.directory/'new'
        first.mkdir(); second.mkdir()
        def fake_score(run, folder, grammar, program, args, threads):
            names = set(r.read(folder/'slice.json')['cards'])
            rows = [row['parse'] for row in self.index.values() if row['name'] in names]
            r.jsonl(folder/'rows.jsonl', rows)
        with patch.object(author.r, 'execute', side_effect=fake_score):
            old = author.early_parse_screen(self.root, first, self.root/'grammar', self.task, self.data)
            new = author.early_parse_screen(self.root, second, self.root/'grammar', self.task, compact=compact)
        self.assertEqual(old, new)
        for name in ('gold.jsonl', 'oracle.json', 'slice.json', 'rows.jsonl'):
            self.assertEqual((first/'parse-screen'/name).read_bytes(), (second/'parse-screen'/name).read_bytes())
        self.assertEqual([(e['ordinal'],e['record'],e['face']) for e in compact['entries']],
                         [(0,0,0),(1,0,1),(2,1,0),(3,1,1),(4,2,0),(5,2,1)])

    def test_packet_missing_tampered_stale_and_swapped_fail_closed(self):
        support = self.make_packet()
        path = self.root/'screen-packet.json'
        original = path.read_bytes()
        self.assertIsNotNone(packet.load(self.root, self.task, 'manifest-hash', support))
        with self.assertRaisesRegex(ValueError, 'task or checkpoint'):
            packet.load(self.root, self.task, 'different-manifest', support)
        with self.assertRaisesRegex(ValueError, 'task or checkpoint'):
            packet.load(self.root, {**self.task, 'id': 'other-task'}, 'manifest-hash', support)
        with self.assertRaisesRegex(ValueError, 'binding'):
            packet.load(self.root, self.task, 'manifest-hash', {'screen_packet_sha256': '0'*64})
        path.write_bytes(original+b' ')
        with self.assertRaisesRegex(ValueError, 'changed'):
            packet.load(self.root, self.task, 'manifest-hash', support)
        path.unlink()
        with self.assertRaisesRegex(ValueError, 'missing'):
            packet.load(self.root, self.task, 'manifest-hash', support)
        path.symlink_to(self.directory)
        with self.assertRaisesRegex(ValueError, 'unsafe'):
            packet.load(self.root, self.task, 'manifest-hash', support)

    def test_outside_full_and_oversize_are_not_silently_truncated(self):
        by_name = {}
        for ordinal, row in self.index.items():
            by_name.setdefault(row['name'], []).append((int(ordinal), row))
        oracle = {}
        for n, record in enumerate(self.oracle):
            oracle.setdefault(tuple(sorted(f['name'] for f in record['cards'])), []).append((n, record))
        full = cohort.close_cohort(by_name, self.records, {'names': self.task['cohort']}, already_closed=True)
        with self.assertRaisesRegex(ValueError, 'escaped full'):
            packet.build(by_name, self.records, oracle, {'id': 'job', 'target_faces': ['OUTSIDE']}, full, 'sha')
        with patch.object(packet, 'MAX_BYTES', 1):
            self.assertIsNone(packet.build(by_name, self.records, oracle,
                                           {'id': 'job', 'target_faces': ['A']}, full, 'sha'))


class PacketWorkflowTests(unittest.TestCase):
    def test_failed_packet_screen_never_loads_full_data_pass_still_runs_full_checks(self):
        for status in ('passed', 'failed'):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as tmp:
                run, root = Path(tmp)/'run', Path(tmp)/'worker'
                for folder in (run/'tasks', run/'inputs', root/'grammar/syntax', root/'families', root/'tests/fixtures'):
                    folder.mkdir(parents=True)
                (root/'grammar/syntax/example.ebnf').write_text('example ::= "example"\n')
                r.write(run/'inputs/manifest.json', {})
                r.write(run/'tasks/task.json', {})
                r.write(run/'support.json', {})
                r.write(root/'families/case.json', {'cases': [{'expected': []}, {'reject': True}, {}, {}]})
                r.write(root/'tests/fixtures/edits.json', [])
                task = {'rules': ['m_seed'], 'fixture': 'families/case.json', 'screen_packet_sha256': 'a'*64,
                        'edits': 'tests/fixtures/edits.json', 'cohort': ['Example'], 'target_faces': ['Example']}
                screen = {'status': status, 'failures': [{'face': 'Example'}] if status == 'failed' else []}
                with patch.object(author, 'verify', return_value=(task, root, {'commit': 'base'})), \
                     patch.object(author, 'seeded', side_effect=[{'m_seed'}, set()]), \
                     patch.object(author, 'check_seeded_files'), patch.object(author, 'apply_hooks', return_value=[]), \
                     patch.object(author, 'helper', return_value=0), \
                     patch.object(packet, 'load', return_value={'bounded': True}), \
                     patch.object(author, 'early_parse_screen', return_value=screen) as early, \
                     patch.object(author.w, 'payload', return_value='full-data') as payload, \
                     patch.object(author.w, 'checks', return_value={'faces': 1, 'failures': []}) as checks, \
                     patch.object(author, 'edited_checks', return_value=[]) as edited, \
                     contextlib.redirect_stdout(io.StringIO()):
                    report = author._check(run, 'task')
                self.assertEqual(report['passed'], status == 'passed')
                self.assertEqual(payload.call_count, int(status == 'passed'))
                self.assertEqual(checks.call_count, int(status == 'passed'))
                self.assertEqual(edited.call_count, int(status == 'passed'))
                self.assertEqual(early.call_args.kwargs['compact'], {'bounded': True})

    def test_verify_checks_existing_author_packet_hash(self):
        with tempfile.TemporaryDirectory() as tmp:
            run, root = Path(tmp)/'run', Path(tmp)/'worker'
            for directory in (run/'tasks', run/'support/runner', root/'grammar/syntax'):
                directory.mkdir(parents=True)
            (run/'support/runner/retire_author.py').write_bytes(Path(author.__file__).read_bytes())
            (run/'packet.json').write_text('{"original":true}\n')
            descriptor = run/'tasks/task.json'
            r.write(descriptor, {'checkout': str(root), 'write_files': [], 'packet': str(run/'packet.json')})
            r.write(run/'support.json', {'files': r.tree(run/'support'),
                    'tasks': {'task': r.digest(descriptor)}, 'packet_sha256': r.digest(run/'packet.json')})
            with patch.object(r, 'verify', return_value={'commit': 'base', 'source': 'compiled'}), \
                 patch.object(author.dev, 'source', return_value='compiled'), \
                 patch.object(author.subprocess, 'check_output', side_effect=['base\n', '']), \
                 patch.object(author, 'changed_files', return_value=[]), \
                 patch.object(author, 'check_root_hooks'):
                author.verify(run, 'task')
            (run/'packet.json').write_text('{"tampered":true}\n')
            with patch.object(r, 'verify', return_value={'commit': 'base', 'source': 'compiled'}):
                with self.assertRaisesRegex(ValueError, 'task packet changed'):
                    author.verify(run, 'task')


class PreparedPacketTests(unittest.TestCase):
    setUp = task_tests.TaskTests.setUp
    tearDown = task_tests.TaskTests.tearDown
    git = task_tests.TaskTests.git
    prepare = task_tests.TaskTests.prepare

    def test_coordinator_cache_refuses_inputs_outside_manifest(self):
        tasker._packet_data.cache_clear()
        (self.inputs/'frozen/index.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'cohort inputs differ'):
            tasker._packet_data(str(self.inputs), r.digest(self.inputs/'manifest.json'))

    def test_large_task_preparation_binds_and_verifies_compact_packet(self):
        frozen = self.inputs/'frozen'
        names = ['A', 'A', *(f'S{i}' for i in range(127))]
        gold = [{'faces': [{'name': name, 'abilities': []}]} for name in names]
        oracle = [{'cards': [{'name': name, 'text': name}]} for name in names]
        index = {str(i): {'name': name, 'text': name, 'record': i, 'face': 0,
                          'parse': {'name': name, 'text': name, 'status': 'equal', 'readings': 1},
                          'flags': [True]*4} for i, name in enumerate(names)}
        r.write(frozen/'index.json', index)
        r.jsonl(frozen/'gold.jsonl', gold)
        r.write(frozen/'oracle.json', oracle)
        manifest = r.read(self.inputs/'manifest.json')
        manifest['files'] = r.tree(frozen)
        r.write(self.inputs/'manifest.json', manifest)
        tasker._packet_data.cache_clear()
        self.cohort = {'names': sorted(set(names)), 'sampled': False}
        result = self.prepare()
        run = Path(result['run'])
        task = r.read(run/'tasks/job-123.json')
        support = r.read(run/'support.json')
        self.assertEqual(r.digest(run/'screen-packet.json'), task['screen_packet_sha256'])
        compact = packet.load(run, task, r.digest(self.inputs/'manifest.json'), support)
        self.assertEqual([(e['ordinal'], e['record'], e['face']) for e in compact['entries']],
                         [(0, 0, 0), (1, 1, 0)])
        self.assertEqual(author.verify(run, 'job-123')[0]['id'], 'job-123')
        (run/'screen-packet.json').write_text('{}')
        with self.assertRaisesRegex(ValueError, 'screen packet changed'):
            author.verify(run, 'job-123')
