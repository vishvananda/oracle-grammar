"""Deletion workers share immutable inputs, not writable grammar or acceptance state."""
from argparse import Namespace
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stdout
from io import StringIO
from unittest.mock import patch

import retire as r
import retire_worker as w


class RetirementTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.run = Path(temporary.name)/'run'
        grammar = self.run/'frozen/grammar/syntax'
        grammar.mkdir(parents=True)
        (self.run/'frozen/grammar/tail').mkdir()
        (grammar/'mined.ebnf').write_text(
            'mined_ability ::= m_a | m_b\n'
            '/* @sem () */\nnever_ability ::= "never"\n'
            '/* @sem A */\nm_a ::= "alpha"\n'
            '/* @sem B */\nm_b ::= "beta"\n')
        for name in ('jobs', 'slots', 'scan'):
            (self.run/name).mkdir()
        r.write(self.run/'manifest.json', {'format': 3, 'slots': 2, 'threads': 1,
                                         'population': 2, 'files': r.tree(self.run/'frozen'),
                                         'tooling': r.tooling()})
        r.write(self.run/'scan/rules.json', {'unused': ['m_a', 'm_b']})
        r.write(self.run/'scan-complete.json', {'report': r.digest(self.run/'scan/rules.json')})
        self.index = {name: {'name': name, 'text': text, 'parse': {'name': name, 'text': text,
                      'status': 'equal', 'readings': 1}, 'flags': [True, True, True, True]}
                      for name, text in [('A', 'Alpha.'), ('B', 'Beta.')]}

    def job(self, rule):
        job = self.run/'jobs'/rule
        job.mkdir()
        r.write(job/'job.json', {'rule': rule,
                                'snapshot': r.digest(self.run/'manifest.json'),
                                'scan': r.digest(self.run/'scan/rules.json')})
        return job

    def perform(self, rule):
        job = self.job(rule)
        with patch.object(w, 'checks', return_value={'faces': 1, 'failures': []}):
            result = w.perform(self.run, job, 'luna-1', (self.index, [], []), r.verify(self.run))
        self.assertEqual(result['status'], 'provisional-pass')
        return job

    def test_jobs_do_not_share_writable_grammar_and_assemble_by_rule_name(self):
        before = r.tree(self.run/'frozen')
        a, b = self.perform('m_a'), self.perform('m_b')
        self.assertEqual(r.tree(self.run/'frozen'), before)
        self.assertIn('m_b ::=', (w.attempt_path(a, r.read(a/'result.json'))/
                                 'grammar/syntax/mined.ebnf').read_text())
        self.assertIn('m_a ::=', (w.attempt_path(b, r.read(b/'result.json'))/
                                 'grammar/syntax/mined.ebnf').read_text())
        r.assemble(Namespace(run=self.run))
        combined = (self.run/'combined/grammar/syntax/mined.ebnf').read_text()
        self.assertIn('mined_ability ::= never_ability', combined)
        self.assertNotIn('m_a ::=', combined)
        self.assertNotIn('m_b ::=', combined)
        self.assertFalse((self.run/'combined/validation.json').exists())

    def test_changed_frozen_input_and_stale_job_are_rejected(self):
        job = self.job('m_a')
        (self.run/'frozen/grammar/syntax/mined.ebnf').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'frozen input changed'):
            r.verify(self.run)
        r.write(self.run/'scan/rules.json', {'unused': ['m_b']})
        with self.assertRaisesRegex(ValueError, 'stale'):
            w.job_inputs(self.run, job)

    def test_a_different_worker_implementation_cannot_reuse_the_campaign(self):
        with patch.object(r, 'tooling', return_value={}):
            with self.assertRaisesRegex(ValueError, 'implementation changed'):
                r.verify(self.run)

    def test_live_worker_blocks_assembly_and_another_claim(self):
        job = self.perform('m_a')
        with r.locked(job/'lease'):
            with r.locked(job/'lease', blocking=False) as acquired:
                self.assertFalse(acquired)
            with self.assertRaisesRegex(ValueError, 'still running'):
                r.assemble(Namespace(run=self.run))
        with r.locked(job/'lease', blocking=False) as acquired:
            self.assertTrue(acquired)

    def test_process_exit_releases_lease_without_a_timeout_or_expiring_token(self):
        path = self.run/'slots/0'
        code = ('import fcntl,sys; f=open(sys.argv[1],"a"); '
                'fcntl.flock(f,fcntl.LOCK_EX); print("locked",flush=True); sys.stdin.read()')
        process = subprocess.Popen([sys.executable, '-c', code, str(path)],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.addCleanup(lambda: process.poll() is None and process.kill())
        self.assertEqual(process.stdout.readline().strip(), 'locked')
        with r.locked(path, blocking=False) as acquired:
            self.assertFalse(acquired)
        process.communicate(timeout=5)
        with r.locked(path, blocking=False) as acquired:
            self.assertTrue(acquired)

    def test_print_loss_is_a_failure_even_when_parse_is_equal(self):
        row = dict(self.index['A']['parse'], exact=True, default=None, round_trip=[8, 8])
        self.assertEqual(w.compare([row], self.index, {'A'}),
                         [{'face': 'A', 'lost': ['default', 'exact_default']}])
        row.pop('default')
        self.assertEqual(w.compare([row], self.index, {'A'}), [])
        row['default'] = 'Another valid spelling.'
        self.assertEqual(w.compare([row], self.index, {'A'}),
                         [{'face': 'A', 'lost': ['exact_default']}])

    def test_unknown_or_branching_cohorts_are_escalated(self):
        p = self.run/'frozen/grammar/syntax/mined.ebnf'
        p.write_text(p.read_text().replace('"alpha"', '("alpha" | "beta")'))
        self.assertEqual(w.cohort(self.run, 'm_a', self.index), [])

    def test_companion_faces_keep_the_original_source_record_names(self):
        data = (self.index, [{'faces': [{'name': 'A'}, {'name': 'B'}]}],
                [{'cards': [{'name': 'A'}, {'name': 'B'}]}])
        directory = self.run/'subset'
        directory.mkdir()
        self.assertEqual(w.subset(directory, ['A'], data), {'A', 'B'})
        self.assertEqual(len(r.rows(directory/'gold.jsonl')[0]['faces']), 2)

    def test_same_name_and_text_occurrences_keep_their_individual_baselines(self):
        from copy import deepcopy
        other = deepcopy(self.index['A'])
        other['parse']['readings'] = 2
        index = {'first': self.index['A'], 'second': other}
        current = [entry['parse'] | {'exact': True} for entry in index.values()]
        self.assertEqual(w.compare(current, index, {'A'}), [])
        with self.assertRaisesRegex(ValueError, 'exact cohort'):
            w.compare(current[:1], index, {'A'})
        current.reverse()
        self.assertEqual(len(w.compare(current, index, {'A'})), 2)

    def test_all_matching_readings_must_retain_each_print_gate(self):
        row = {'matching_readings': [0, 2], 'printing': [
            {'reading': 0, 'exact': True},
            {'reading': 2, 'exact': False, 'default': None, 'round_trip': [8, 0]}]}
        self.assertEqual(r.flags(row), [False, False, False, False])
        row['printing'][1] = {'reading': 2, 'exact': True, 'default': 'Other valid words.'}
        self.assertEqual(r.flags(row), [True, True, True, False])
        row['printing'].pop()
        with self.assertRaisesRegex(ValueError, 'coverage'):
            r.flags(row)

    def test_worker_receipts_and_reports_do_not_depend_on_retaining_a_tool_handle(self):
        job = self.job('m_a')
        with patch.object(w, 'payload', return_value=(self.index, [], [])), \
                patch.object(w, 'checks', return_value={'faces': 1, 'failures': []}), \
                redirect_stdout(StringIO()):
            w.worker(Namespace(run=self.run, name='luna', limit=1, rules=['m_a']))
        receipts = list((self.run/'workers').glob('*.json'))
        self.assertEqual(len(receipts), 1)
        receipt = r.read(receipts[0])
        self.assertEqual(receipt['status'], 'completed')
        self.assertEqual(receipt['jobs'][0]['rule'], 'm_a')
        self.assertGreaterEqual(receipt['wall_seconds'], 0)
        self.assertGreaterEqual(receipt['finished_unix'], receipt['started_unix'])
        result = r.read(job/'result.json')
        self.assertEqual(result['artifacts']['scores'],
                         str((w.attempt_path(job, result)/'rows.jsonl').relative_to(self.run)))
        output = StringIO()
        with redirect_stdout(output):
            w.report(Namespace(run=self.run, rules=['m_a']))
        self.assertEqual(json.loads(output.getvalue())['jobs'][0]['status'], 'provisional-pass')

    def test_freeze_keeps_duplicate_occurrences_and_runs_without_mutating_its_runner(self):
        source = self.run/'frozen'
        inputs = self.run/'inputs'
        inputs.mkdir()
        helpers = inputs/'helpers'
        helpers.mkdir()
        for name in r.HELPERS:
            (helpers/name).write_text('fixture helper: ' + name)
        r.write(helpers/'build.json', {'source_sha256': 'fixture-source',
                                     'binaries': {n: r.digest(helpers/n) for n in r.HELPERS}})
        first = dict(self.index['A']['parse'])
        second = dict(first, readings=2)
        r.jsonl(inputs/'gold.jsonl', [{'faces': [{'name': 'A'}]}, {'faces': [{'name': 'A'}]}])
        r.write(inputs/'oracle.json', [{'cards': [{'name': 'A'}]}, {'cards': [{'name': 'A'}]}])
        r.write(inputs/'allowed.json', {'cards': ['A']})
        r.jsonl(inputs/'parse.jsonl', [first, second])
        r.jsonl(inputs/'prints.jsonl', [dict(first, exact=True),
                                       dict(second, exact=True, default='Other valid spelling.')])
        target = self.run/'campaign'
        args = Namespace(run=target, source=source, helpers=helpers,
                         gold=inputs/'gold.jsonl', oracle=inputs/'oracle.json',
                         allowed=inputs/'allowed.json', parse=inputs/'parse.jsonl',
                         prints=[inputs/'prints.jsonl'], slots=2, threads=1)
        with patch.object(r.dev, 'source', return_value='fixture-source'), \
                patch.object(r.subprocess, 'check_output', side_effect=[b'', 'fixture-commit\n']), \
                redirect_stdout(StringIO()):
            r.freeze(args)
        manifest = r.verify(target)
        self.assertEqual((manifest['population'], manifest['names']), (2, 1))
        entries = list(r.read(target/'frozen/index.json').values())
        self.assertEqual([(e['record'], e['face']) for e in entries], [(0, 0), (1, 0)])
        self.assertTrue(entries[0]['flags'][3])
        self.assertFalse(entries[1]['flags'][3])
        before = r.tree(target/'frozen')
        subprocess.run([sys.executable, str(target/'frozen/runner/retire.py'), 'status', str(target)],
                       check=True, capture_output=True, text=True)
        self.assertEqual(r.tree(target/'frozen'), before)

    def test_a_modified_worker_candidate_is_not_assembled(self):
        job = self.perform('m_a')
        (w.attempt_path(job, r.read(job/'result.json'))/'grammar/syntax/mined.ebnf').write_text('changed')
        with self.assertRaisesRegex(ValueError, 'candidate changed'):
            r.assemble(Namespace(run=self.run))

    def test_assembly_closes_the_queue_before_combined_validation(self):
        job = self.job('m_a')
        (self.run/'combined').mkdir()
        with patch.object(w, 'payload', return_value=(self.index, [], [])), \
                patch.object(w, 'perform') as perform:
            w.worker(Namespace(run=self.run, name='luna', limit=1))
        perform.assert_not_called()
        self.assertFalse((job/'result.json').exists())

    def test_helpers_inherit_leases_and_attempts_never_reuse_output_paths(self):
        directory = self.run/'command'
        directory.mkdir()
        with r.locked(self.run/'slots/0'), patch.object(r.subprocess, 'run') as execute:
            execute.return_value.returncode = 0
            r.execute(self.run, directory, self.run/'frozen/grammar', 'score', [], 1)
            self.assertEqual(execute.call_args.kwargs['pass_fds'], tuple(r._LEASES))
            self.assertTrue(execute.call_args.kwargs['pass_fds'])
        self.assertGreaterEqual(r.rows(directory/'commands.jsonl')[0]['wall_seconds'], 0)
        job = self.perform('m_a')
        old = w.attempt_path(job, r.read(job/'result.json'))
        with patch.object(w, 'checks', return_value={'faces': 1, 'failures': []}):
            w.perform(self.run, job, 'luna-2', (self.index, [], []), r.verify(self.run))
        new = w.attempt_path(job, r.read(job/'result.json'))
        self.assertNotEqual(old, new)
        self.assertTrue(old.exists() and new.exists())

    def test_an_orphaned_helper_keeps_the_job_locked_until_it_exits(self):
        path = self.run/'slots/0'
        code = ('import sys,subprocess; sys.path.insert(0,sys.argv[1]); import retire as r; '
                'exec("with r.locked(sys.argv[2]):\\n "'
                '"p=subprocess.Popen([sys.executable,\'-c\',\'import time; time.sleep(30)\'],"'
                '"pass_fds=tuple(r._LEASES),stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL); "'
                '"print(p.pid,flush=True); sys.stdin.read()")')
        parent = subprocess.Popen([sys.executable, '-c', code, str(Path(r.__file__).parent), str(path)],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        child = None
        try:
            child = int(parent.stdout.readline().strip())
            parent.kill()
            parent.communicate(timeout=5)
            with r.locked(path, blocking=False) as acquired:
                self.assertFalse(acquired)
        finally:
            if parent.poll() is None:
                parent.kill()
                parent.communicate(timeout=5)
            if child is not None:
                try:
                    os.kill(child, 15)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 3
        while True:
            with r.locked(path, blocking=False) as acquired:
                if acquired:
                    break
            self.assertLess(time.monotonic(), deadline)
            time.sleep(0.01)


if __name__ == '__main__':
    unittest.main()
