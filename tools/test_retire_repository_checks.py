import contextlib
import fcntl
import io
import json
import os
from pathlib import Path
import tempfile
import sys
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import dev as source_dev
import retire_repository_checks as adapter


class FakeDev:
    FAST_GATES = (('fmt', ['fmt-command']), ('pin', ['pin-command']),
                  ('hygiene', ['hygiene-command']), ('tools', ['tools-command']))
    CLIPPY = ['clippy-source-command']

    def __init__(self, root):
        self.ROOT = root
        self.WORK = root/'.cache/dev'
        self.WORK.mkdir(parents=True)
        (self.WORK/'build.json').write_text(json.dumps({'binaries': {'helper': 'abc'}}))
        self.pin = self
        self.calls = []
        self.cache = {}
        self.failures = set()

    def check_model(self):
        self.calls.append(('pin-check', os.environ.get('ORACLE_GRAMMAR_DIR')))

    def threads(self):
        return int(os.environ.get('ORACLE_GRAMMAR_THREADS', '2'))

    def build_environment(self):
        return {**os.environ, 'CARGO_INCREMENTAL': '1'}

    def passed(self, name, extra):
        self.calls.append(('receipt', name, dict(extra), os.environ.get('ORACLE_GRAMMAR_DIR')))
        return self.cache.get(name, False)

    def Gate(self, name, command, environment):
        self.calls.append(('gate', name, list(command), dict(environment)))
        if name in self.failures:
            (self.WORK/'check'/f'{name}.log').write_text('gate failure evidence\n')
        status = 'FAILED' if name in self.failures else 'passed'
        class Result:
            def wait(self):
                return name, status, 0.1
        return Result()


class AdapterTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root/'grammar').mkdir()
        self.dev = FakeDev(self.root)
        mock_factory = patch.object(adapter, 'inherited_gate_factory', side_effect=lambda dev, fds: dev.Gate)
        mock_factory.start()
        self.addCleanup(mock_factory.stop)

    def run_check(self, force=False):
        with contextlib.redirect_stdout(io.StringIO()) as output:
            results = adapter.check(self.root, force=force, dev=self.dev)
        return results, output.getvalue()

    def test_fast_gates_are_clean_and_native_gates_bind_candidate(self):
        before = {'ORACLE_GRAMMAR_DIR': '/wrong/check-out/grammar', 'ORACLE_GRAMMAR_THREADS': '7',
                  'ORACLE_GRAMMAR_FOO': 'unrelated', 'CARGO_TARGET_DIR': '/shared/cache'}
        with patch.dict(os.environ, before, clear=True):
            results, output = self.run_check()
            self.assertEqual(dict(os.environ), before)
        gates = {call[1]: call for call in self.dev.calls if call[0] == 'gate'}
        self.assertEqual(len(results), 8)
        self.assertEqual([name for name in ('fmt', 'pin', 'hygiene', 'tools') if name in output],
                         ['fmt', 'pin', 'hygiene', 'tools'])
        for name in ('fmt', 'pin', 'hygiene', 'tools'):
            self.assertFalse(any(k.startswith('ORACLE_GRAMMAR_') for k in gates[name][3]))
            self.assertEqual(gates[name][2], dict(self.dev.FAST_GATES)[name])
        for name in ('clippy', 'build', 'families', 'test'):
            env = gates[name][3]
            self.assertEqual(env['ORACLE_GRAMMAR_DIR'], str(self.root/'grammar'))
            self.assertEqual(env['ORACLE_GRAMMAR_THREADS'], '7')
            self.assertEqual(env['CARGO_TARGET_DIR'], '/shared/cache')
            self.assertNotIn('ORACLE_GRAMMAR_FOO', env)
        self.assertEqual(gates['clippy'][2], self.dev.CLIPPY)
        self.assertEqual([call[3] for call in self.dev.calls if call[0] == 'receipt'],
                         [str(self.root/'grammar')]*2)

    def test_receipts_reuse_only_when_candidate_context_is_active(self):
        self.dev.cache = {'families': True, 'test': True}
        results, _ = self.run_check()
        self.assertEqual({name for name, status, _ in results if status == 'reused'}, {'families', 'test'})
        self.assertFalse(any(c[0] == 'gate' and c[1] in ('families', 'test') for c in self.dev.calls))
        self.assertTrue(all(c[3] == str(self.root/'grammar') for c in self.dev.calls if c[0] == 'receipt'))
        self.dev.calls.clear()
        self.run_check(force=True)
        self.assertFalse(any(c[0] == 'receipt' for c in self.dev.calls))
        self.assertEqual({c[1] for c in self.dev.calls if c[0] == 'gate'} & {'families', 'test'},
                         {'families', 'test'})

    def test_actual_receipt_key_distinguishes_candidate_from_inherited_grammar(self):
        with patch.object(source_dev, 'compiler', return_value={'environment': {}, 'versions': []}):
            with patch.dict(os.environ, {'ORACLE_GRAMMAR_DIR': str(self.root/'grammar')}):
                candidate = source_dev.receipt_key('test', 'tree-hash', {})
            with patch.dict(os.environ, {'ORACLE_GRAMMAR_DIR': '/old/grammar'}):
                old = source_dev.receipt_key('test', 'tree-hash', {})
                self.assertNotEqual(candidate, old)
                def cached(name, extra):
                    return name == 'test' and source_dev.receipt_key(name, 'tree-hash', extra) == candidate
                with patch.object(self.dev, 'passed', side_effect=cached):
                    results, _ = self.run_check()
            self.assertIn(('test', 'reused', 0.0), results)

    def test_failure_reports_log_and_prevents_success(self):
        self.dev.failures = {'tools'}
        with contextlib.redirect_stdout(io.StringIO()) as output:
            with self.assertRaisesRegex(ValueError, 'tools'):
                adapter.check(self.root, dev=self.dev)
        self.assertIn('gate failure evidence', output.getvalue())

    def test_build_failure_skips_family_but_runs_test(self):
        self.dev.failures = {'build'}
        with contextlib.redirect_stdout(io.StringIO()) as output:
            with self.assertRaisesRegex(ValueError, 'build'):
                adapter.check(self.root, dev=self.dev)
        self.assertIn('families  skipped', output.getvalue())
        self.assertTrue(any(c[0] == 'gate' and c[1] == 'test' for c in self.dev.calls))


class LeaseTests(unittest.TestCase):
    def test_descriptor_validation_rejects_duplicates_closed_and_invalid(self):
        with tempfile.TemporaryFile() as f:
            fd = f.fileno()
            self.assertEqual(adapter.lease_descriptors(str(fd)), (fd,))
            for invalid in (f'{fd},{fd}', '0', '-1', '1,a', (fd, fd)):
                with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                    adapter.lease_descriptors(invalid)
        with self.assertRaisesRegex(ValueError, 'not open'):
            adapter.lease_descriptors((fd,))

    def test_child_holds_lock_when_parent_closes_its_descriptor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/'check').mkdir()
            lock = root/'lease'
            fd = os.open(lock, os.O_CREAT | os.O_RDWR, 0o600)
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            dev = SimpleNamespace(Gate=source_dev.Gate, WORK=root, ROOT=root)
            gate_type = adapter.inherited_gate_factory(dev, adapter.lease_descriptors((fd,)))
            ready, release = root/'ready', root/'release'
            script = ('import pathlib,time,sys; '
                      'pathlib.Path(sys.argv[1]).touch(); '
                      'p=pathlib.Path(sys.argv[2]); '
                      'exec("while not p.exists():\\n time.sleep(0.01)")')
            gate = gate_type('lease-smoke', [sys.executable, '-c', script, str(ready), str(release)])
            try:
                limit = time.monotonic()+5
                while not ready.exists() and time.monotonic() < limit:
                    time.sleep(0.01)
                self.assertTrue(ready.exists())
                os.close(fd)
                fd = None
                contender = os.open(lock, os.O_RDWR)
                try:
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    release.touch()
                    self.assertEqual(gate.wait()[1], 'passed')
                    fcntl.flock(contender, fcntl.LOCK_EX | fcntl.LOCK_NB)
                finally:
                    os.close(contender)
            finally:
                release.touch()
                gate.wait()
                if fd is not None:
                    os.close(fd)


if __name__ == '__main__':
    unittest.main()
