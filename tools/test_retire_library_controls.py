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

    def test_setup_normalizes_json_source_and_path_before_compiled_source_check(self):
        observed = []
        def source_hash(source):
            self.assertIsInstance(source, Path)
            observed.append(source)
            return 'a'*64
        with patch.object(controls.dev, 'source', side_effect=source_hash):
            for source in (self.source, str(self.source)):
                self.assertEqual(controls.setup(self.cp, source)['source_sha256'], 'a'*64)
        self.assertEqual(observed, [self.source, self.source])

    def test_stage_passes_json_checkpoint_source_through_real_setup(self):
        import retire_batch as batch
        subprocess.run(['git', 'init', '-q', str(self.source)], check=True)
        (self.grammar/'example.ebnf').write_text('example')
        (self.source/'tests').mkdir()
        (self.source/'tests/families.json').write_text('[]')
        subprocess.run(['git', 'add', '.'], cwd=self.source, check=True)
        subprocess.run(['git', '-c', 'user.name=Test', '-c', 'user.email=t@x',
                        'commit', '-qm', 'base'], cwd=self.source, check=True)
        commit = batch._git(self.source, 'rev-parse', 'HEAD')
        r.write(self.inputs/'manifest.json', {'commit': commit, 'files': {'build.json': r.digest(self.build)}})
        cp = dict(self.cp, commit=commit, source=str(self.source))
        def source_hash(source):
            self.assertIsInstance(source, Path)
            return 'a'*64
        with patch.object(controls.dev, 'source', side_effect=source_hash), \
                patch.object(controls, 'check', side_effect=ValueError('baseline sentinel')):
            manifest = batch.stage(self.root, cp, [{'job_id': 'j'}])
        self.assertEqual(manifest['failure_scope'], 'baseline-library-control')
        self.assertEqual(manifest['reason'], 'baseline sentinel')

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

    def test_unmatched_group_rechecks_the_original_isolated_tests(self):
        control = controls.setup(self.cp, self.source)
        def run(command, **kwargs):
            log = kwargs['stdout']
            if '--test-threads' in command:
                log.write('running 2 tests\ntest tests::one ... ok\ntest tests::extra ... ok\n'
                          'test result: ok. 2 passed; 0 failed;\n')
                return subprocess.CompletedProcess(command, 0)
            name = command[2]
            if name == 'tests::two':
                log.write(f'running 1 test\ntest {name} ... FAILED\n')
                return subprocess.CompletedProcess(command, 101)
            log.write(f'running 1 test\ntest {name} ... ok\ntest result: ok. 1 passed; 0 failed;\n')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(controls.subprocess, 'run', side_effect=run) as invoked:
            with self.assertRaisesRegex(ValueError, 'control failed'):
                controls.check(self.cp, control, self.grammar, self.root/'veto')
        self.assertEqual(invoked.call_count, 3)
        result = r.read(self.root/'veto/result.json')
        self.assertEqual([(row['test'], row['passed'], Path(row['log']).name) for row in result['tests']],
                         [('tests::one', True, 'test-000.log'), ('tests::two', False, 'test-001.log')])

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
            self.assertEqual(command[1:], ['--exact', 'tests::one', 'tests::two', '--test-threads', '1'])
            kwargs['stdout'].write('running 2 tests\ntest tests::two ... ok\ntest tests::one ... ok\n'
                                   'test result: ok. 2 passed; 0 failed;\n')
            return subprocess.CompletedProcess(command, 0)
        with patch.object(controls.subprocess, 'run', side_effect=run):
            result = controls.check(self.cp, control, self.grammar, self.root/'pass')
        self.assertTrue(result['passed'])
        self.assertEqual(len(calls), 1)
        self.assertEqual([item['test'] for item in result['tests']], control['tests'])
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
