import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import retire_library_controls as controls
import retire as r


class LibraryControlsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root/'source'
        self.source.mkdir()
        self.grammar = self.source/'grammar'
        self.grammar.mkdir()
        self.inputs = self.root/'inputs'
        (self.inputs/'frozen').mkdir(parents=True)
        self.build = self.inputs/'frozen/build.json'
        self.compiler = {'versions': ['rustc', 'cargo']}
        self.build.write_text(json.dumps({'source_sha256': 'a'*64, 'compiler': self.compiler}))
        (self.inputs/'manifest.json').write_text(json.dumps({'commit': 'base', 'files': {'build.json': r.digest(self.build)}}))
        self.binary = self.root/'test-bin'
        self.binary.write_bytes(b'binary')
        self.profile = self.root/'profile.json'
        self.document = {'source_sha256': 'a'*64, 'compiler': self.compiler,
                         'binary': {'path': str(self.binary), 'sha256': r.digest(self.binary)},
                         'tests': ['tests::one', 'tests::two']}
        self.write_profile()
        self.cp = {'commit': 'base', 'inputs': str(self.inputs), 'cpu_pool': str(self.root/'cpu'),
                   'library_regression_controls': {'path': str(self.profile), 'sha256': r.digest(self.profile)}}
        self.source_patch = patch.object(controls.dev, 'source', return_value='a'*64)
        self.source_patch.start()
        self.addCleanup(self.source_patch.stop)

    def write_profile(self):
        self.profile.write_text(json.dumps(self.document))

    def test_source_compiler_and_file_tampering_fail_closed(self):
        control = controls.setup(self.cp, self.source)
        self.profile.write_text('{}')
        with self.assertRaises(ValueError):
            controls.verify(control)
        self.write_profile()
        self.binary.write_bytes(b'changed')
        with self.assertRaises(ValueError):
            controls.verify(control)
        self.binary.write_bytes(b'binary')
        for field, value in [('source_sha256', 'b'*64), ('compiler', {'versions': ['other']})]:
            with self.subTest(field=field):
                saved = self.document[field]
                self.document[field] = value
                self.write_profile()
                self.cp['library_regression_controls']['sha256'] = r.digest(self.profile)
                with self.assertRaises(ValueError):
                    controls.setup(self.cp, self.source)
                self.document[field] = saved
        self.write_profile()
        self.cp['library_regression_controls']['sha256'] = r.digest(self.profile)
        self.binary.unlink()
        self.binary.symlink_to(self.build)
        with self.assertRaises(ValueError):
            controls.setup(self.cp, self.source)

    def test_exact_test_invocations_two_cpu_tokens_and_zero_tests_fail(self):
        control = controls.setup(self.cp, self.source)
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            self.assertEqual(len(r._LEASES), 2)
            self.assertEqual(kwargs['env']['ORACLE_GRAMMAR_DIR'], str(self.grammar))
            self.assertEqual(kwargs['env']['RUST_TEST_THREADS'], '2')
            for fd in kwargs['pass_fds']:
                os.fstat(fd)
            test = command[2]
            kwargs['stdout'].write(f'running 1 test\ntest {test} ... ok\ntest result: ok. 1 passed; 0 failed;\n')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(controls.subprocess, 'run', side_effect=run):
            result = controls.check(self.cp, control, self.grammar, self.root/'pass')
        self.assertTrue(result['passed'])
        self.assertEqual([call[2] for call in calls], ['tests::one', 'tests::two'])
        def zero(command, **kwargs):
            kwargs['stdout'].write('running 0 tests\ntest result: ok. 0 passed; 0 failed;\n')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(controls.subprocess, 'run', side_effect=zero):
            with self.assertRaisesRegex(ValueError, 'control failed'):
                controls.check(self.cp, control, self.grammar, self.root/'zero')
        self.assertFalse(r.read(self.root/'zero/result.json')['passed'])
        self.assertEqual(r._LEASES, [])


if __name__ == '__main__':
    unittest.main()
