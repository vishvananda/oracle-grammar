import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest

import retire as r
import retire_cpu
import retire_helper


class HelperTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.run = Path(self.tmp.name)/'run'
        self.pool = Path(self.tmp.name)/'cpu'
        support = self.run/'support'
        for directory in ('bin', 'native', 'runner'):
            (support/directory).mkdir(parents=True)
        self.wrapper = support/'bin/probe'
        shutil.copy2(Path(retire_helper.__file__), self.wrapper)
        self.wrapper.chmod(0o755)
        native = support/'native/probe'
        native.write_text('#!/usr/bin/env python3\nimport os,sys\nprint(os.environ.get("ORACLE_GRAMMAR_THREADS"), *sys.argv[1:])\n')
        native.chmod(0o755)
        source = Path(retire_helper.__file__).parent
        for name in (*r.TOOLING, 'retire_cpu.py'):
            shutil.copy2(source/name, support/'runner'/name)
        (self.run/'tasks').mkdir()
        descriptor = self.run/'tasks/one.json'
        r.write(descriptor, {'cpu_pool': str(self.pool), 'helper_threads': 2, 'cpu_capacity': 2})
        r.write(self.run/'support.json', {'tasks': {'one': r.digest(descriptor)}, 'files': r.tree(support)})

    def tearDown(self):
        self.tmp.cleanup()

    def test_direct_helper_waits_for_atomic_cpu_lease(self):
        with retire_cpu.reserve(self.pool, 2, 2):
            process = subprocess.Popen([str(self.wrapper), 'example'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            time.sleep(.3)
            self.assertIsNone(process.poll())
        output, error = process.communicate(timeout=5)
        self.assertEqual(process.returncode, 0, error)
        self.assertEqual(output.strip(), '2 example')

    def test_cli_does_not_mutate_frozen_support_tree(self):
        support = self.run/'support'
        before = r.tree(support)
        env = dict(os.environ)
        env.pop('PYTHONDONTWRITEBYTECODE', None)
        result = subprocess.run([str(self.wrapper), 'check'], env=env, capture_output=True,
                                text=True, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(r.tree(support), before)
        self.assertFalse((support/'runner/__pycache__').exists())

    def test_nested_helper_reuses_inherited_lease_without_deadlock(self):
        with retire_cpu.reserve(self.pool, 2, 2) as fds:
            env = dict(os.environ, RETIRE_CPU_LEASE_FDS=','.join(map(str, fds)),
                       RETIRE_HELPER_EXTRA_FDS=','.join(map(str, fds)))
            result = subprocess.run([str(self.wrapper), 'nested'], env=env, pass_fds=fds,
                                    capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), '2 nested')
            with self.assertRaises(BlockingIOError):
                with retire_cpu.reserve(self.pool, 1, 2, wait=False):
                    pass

    def test_tampered_native_refused(self):
        (self.run/'support/native/probe').write_text('modified')
        result = subprocess.run([str(self.wrapper)], capture_output=True, text=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('native helper changed', result.stderr)


if __name__ == '__main__':
    unittest.main()
