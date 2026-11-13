"""Protect the fast loop: syntax/reference changes do not invalidate binaries."""
from pathlib import Path
import shutil
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import dev


class BuildInputTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        shutil.copytree(dev.ROOT/'src', self.root/'src')
        shutil.copytree(dev.ROOT/'crates', self.root/'crates')
        for name in ('Cargo.toml', 'Cargo.lock', 'pins/model.json', '.cargo/config.toml',
                     'rust-toolchain.toml', 'clippy.toml', 'grammar/lexical.ebnf',
                     'grammar/print-model.txt', 'grammar/print-weights.txt',
                     'tools/dev.py', 'tools/pin.py', 'adapters/mtgish/reference.json',
                     'grammar/syntax/actions.ebnf'):
            destination = self.root/name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(dev.ROOT/name, destination)
        self.before = dev.source(self.root)

    def append(self, name):
        path = self.root/name
        path.write_text(path.read_text()+'\n ')

    def test_runtime_syntax_changes_do_not_require_recompilation(self):
        self.append('grammar/syntax/actions.ebnf')
        self.assertEqual(dev.source(self.root), self.before)

    def test_new_and_removed_syntax_modules_are_explicit_overrides(self):
        path = self.root/'grammar/syntax/extra.ebnf'
        path.write_text('extra ::= "word"')
        self.assertIn(f'--module=extra={path}', dev.modules(self.root))
        (self.root/'grammar/syntax/actions.ebnf').unlink()
        self.assertIn('--module=actions=/dev/null', dev.modules(self.root))
        self.assertEqual(dev.source(self.root), self.before)

    def test_reference_updates_do_not_require_recompilation(self):
        self.append('adapters/mtgish/reference.json')
        self.assertEqual(dev.source(self.root), self.before)

    def test_model_and_lexical_changes_require_recompilation(self):
        self.append('pins/model.json')
        self.assertNotEqual(dev.source(self.root), self.before)
        self.before = dev.source(self.root)
        self.append('grammar/lexical.ebnf')
        self.assertNotEqual(dev.source(self.root), self.before)

    def test_owned_model_sources_and_embedded_assets_require_recompilation(self):
        for name in ('crates/oracle-ir/src/ir.rs',
                     'crates/oracle-ir-contract/src/validate.rs',
                     'crates/oracle-ir-contract/rewrites/primitives.json5',
                     'crates/oracle-ir/schema/190.json'):
            before = dev.source(self.root)
            self.append(name)
            self.assertNotEqual(dev.source(self.root), before, name)

    def test_cache_location_does_not_change_compiler_identity(self):
        with patch.dict('os.environ', {'CARGO_TARGET_DIR': '/tmp/cache-a'}):
            first = dev.compiler()
        with patch.dict('os.environ', {'CARGO_TARGET_DIR': '/tmp/cache-b'}):
            self.assertEqual(dev.compiler(), first)
        with patch.dict('os.environ', {'RUSTFLAGS': '-C opt-level=3'}):
            self.assertNotEqual(dev.compiler(), first)

    def test_warm_helper_build_does_not_invoke_cargo(self):
        with patch.object(dev, 'ready', return_value={'profile': 'quick'}), \
             patch.object(dev.subprocess, 'run', side_effect=AssertionError('unexpected Cargo call')), \
             patch.object(dev.subprocess, 'Popen', side_effect=AssertionError('unexpected Cargo call')), \
             patch('builtins.print'):
            dev.build()

    def test_rebuild_remembers_target_and_respects_explicit_overrides(self):
        work = self.root/'work'
        work.mkdir()
        target = self.root/'shared-target'
        target.mkdir()
        (work/'build.json').write_text(json.dumps({'target_directory': str(target)}))
        clean = {k: v for k, v in os.environ.items() if k != 'CARGO_TARGET_DIR'}
        with patch.object(dev, 'ROOT', self.root), patch.object(dev, 'WORK', work), \
             patch.dict(os.environ, clean, clear=True):
            self.assertEqual(dev.build_environment()['CARGO_TARGET_DIR'], str(target))
            with patch.dict(os.environ, {'CARGO_TARGET_DIR': '/tmp/explicit-target'}):
                self.assertEqual(dev.build_environment()['CARGO_TARGET_DIR'], '/tmp/explicit-target')
            target.rmdir()
            with self.assertRaisesRegex(ValueError, 'target directory is missing'):
                dev.build_environment()
            with patch.dict(os.environ, {'CARGO_TARGET_DIR': str(self.root/'new-target')}):
                self.assertEqual(dev.build_environment()['CARGO_TARGET_DIR'], str(self.root/'new-target'))
            config = self.root/'.cargo/config.toml'
            config.write_text(config.read_text().replace('jobs = 2', 'jobs = 2\ntarget-dir = "custom-target"'))
            self.assertNotIn('CARGO_TARGET_DIR', dev.build_environment())

    def test_quick_builds_and_tests_compile_incrementally_unless_overridden(self):
        clean = {k: v for k, v in os.environ.items() if k != 'CARGO_INCREMENTAL'}
        with patch.dict(os.environ, {**clean, 'CARGO_TARGET_DIR': '/tmp/explicit-target'}, clear=True):
            self.assertEqual(dev.build_environment()['CARGO_INCREMENTAL'], '1')
            self.assertNotIn('CARGO_INCREMENTAL', dev.build_environment(incremental=False))
            with patch.dict(os.environ, {'CARGO_INCREMENTAL': '0'}):
                self.assertEqual(dev.build_environment()['CARGO_INCREMENTAL'], '0')

    def test_documented_test_entrypoint_displays_cargo_help(self):
        result = subprocess.run([sys.executable, str(dev.ROOT/'tools/dev.py'), 'test', '--help'],
                                cwd=dev.ROOT, env={**os.environ, 'CARGO_TERM_COLOR': 'never'},
                                capture_output=True, text=True, check=True, timeout=15)
        self.assertIn('Usage: cargo test', result.stdout)

    def test_test_preset_forwards_filter_and_cache_environment(self):
        environment = {'CARGO_TARGET_DIR': '/tmp/remembered-target'}
        with patch.object(dev, 'build_environment', return_value=environment), \
             patch.object(dev.subprocess, 'run') as run:
            dev.test(['tests::revealed_followups'])
        command = run.call_args.args[0]
        self.assertEqual(command[:2], ['cargo', 'test'])
        self.assertIn('--release', command)
        self.assertIn('--lib', command)
        self.assertEqual(command[-1], 'tests::revealed_followups')
        self.assertEqual(run.call_args.kwargs['env'], environment)

    def test_readers_share_the_lock_and_block_a_build(self):
        code = ("import sys; from pathlib import Path; "
                "sys.path.insert(0, sys.argv[1]); import dev; "
                "dev.WORK = Path(sys.argv[2]); "
                "shared = sys.argv[3] == 'reader'; "
                "exec('with dev.lock(shared=shared): print(123, flush=True)')")
        args = [sys.executable, '-c', code, str(dev.ROOT/'tools'), str(self.root/'work')]
        with patch.object(dev, 'WORK', self.root/'work'), dev.lock(shared=True):
            result = subprocess.run([*args, 'reader'], capture_output=True, text=True, timeout=5, check=True)
            self.assertEqual(result.stdout.strip(), '123')
            with self.assertRaises(subprocess.TimeoutExpired):
                subprocess.run([*args, 'builder'], capture_output=True, timeout=0.3, check=True)
        result = subprocess.run([*args, 'builder'], capture_output=True, text=True, timeout=5, check=True)
        self.assertEqual(result.stdout.strip(), '123')

    def test_compiler_input_changes_require_recompilation(self):
        for name in ('src/model.rs', 'Cargo.lock', 'tools/dev.py', 'rust-toolchain.toml'):
            with self.subTest(name=name):
                before = dev.source(self.root)
                self.append(name)
                self.assertNotEqual(dev.source(self.root), before)



class ReceiptTests(unittest.TestCase):
    """A passed gate is reused only for exactly the files and compiler it passed on."""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for name in ('src/lib.rs', 'grammar/syntax/card.ebnf', 'tests/families.json',
                     'docs/notes.md', 'README.md', 'tools/corpus.py', 'tools/dev.py', '.gitignore'):
            path = self.root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('ignored/\n' if name == '.gitignore' else name)
        subprocess.run(['git', 'init', '-q'], cwd=self.root, check=True)
        subprocess.run(['git', 'add', '.'], cwd=self.root, check=True)
        self.work = self.root/'ignored/work'
        self.work.mkdir(parents=True)
        patches = [patch.object(dev, 'ROOT', self.root), patch.object(dev, 'WORK', self.work),
                   patch.object(dev, 'compiler', return_value={'environment': {}, 'versions': []})]
        for active in patches:
            active.start()
            self.addCleanup(active.stop)

    def edit(self, name):
        path = self.root/name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(path.read_text()+' ' if path.exists() else name)

    def test_documentation_ci_and_other_tools_are_not_gate_inputs(self):
        before = dev.tree(self.root)
        for name in ('docs/notes.md', 'README.md', 'tools/corpus.py', '.github/workflows/ci.yml',
                     'ignored/output.json'):
            self.edit(name)
            self.assertEqual(dev.tree(self.root), before, name)

    def test_sources_grammar_fixtures_new_files_and_the_gate_tool_are_inputs(self):
        for name in ('src/lib.rs', 'grammar/syntax/card.ebnf', 'tests/families.json',
                     'src/new.rs', 'tools/dev.py'):
            before = dev.tree(self.root)
            self.edit(name)
            self.assertNotEqual(dev.tree(self.root), before, name)
        before = dev.tree(self.root)
        (self.root/'src/new.rs').unlink()
        self.assertNotEqual(dev.tree(self.root), before)

    def test_a_pass_is_reused_until_an_input_or_the_compiler_changes(self):
        with dev.receipt('test', {}):
            pass
        self.assertTrue(dev.passed('test', {}))
        self.assertFalse(dev.passed('families', {}))
        self.assertFalse(dev.passed('test', {'helpers': {'probe': 'other'}}))
        with patch.object(dev, 'compiler', return_value={'environment': {'RUSTFLAGS': '-O'}, 'versions': []}):
            self.assertFalse(dev.passed('test', {}))
        with patch.object(dev, 'compiler', return_value={'environment': {'CARGO_INCREMENTAL': '0'},
                                                         'versions': []}):
            self.assertTrue(dev.passed('test', {}))
        self.edit('grammar/syntax/card.ebnf')
        self.assertFalse(dev.passed('test', {}))

    def test_a_failure_or_an_edit_during_the_gate_records_nothing(self):
        with self.assertRaises(subprocess.CalledProcessError), dev.receipt('test', {}):
            raise subprocess.CalledProcessError(101, 'cargo')
        self.assertFalse(dev.passed('test', {}))
        with patch('builtins.print'), dev.receipt('test', {}):
            self.edit('src/lib.rs')
        self.assertFalse(dev.passed('test', {}))

    def test_no_receipts_outside_a_git_checkout(self):
        shutil.rmtree(self.root/'.git')
        self.assertIsNone(dev.tree(self.root))
        with patch('builtins.print'), dev.receipt('test', {}):
            pass
        self.assertFalse(dev.passed('test', {}))

    def test_an_unfiltered_test_run_records_its_pass_and_uses_the_thread_budget(self):
        real = subprocess.run
        with patch.object(dev, 'build_environment', return_value={}), \
             patch.object(dev.pin, 'check_model'), \
             patch.dict(os.environ, {'ORACLE_GRAMMAR_THREADS': '5'}), \
             patch.object(dev.subprocess, 'run', wraps=dev.subprocess.run) as run:
            run.side_effect = lambda command, **kwargs: (
                dev.subprocess.CompletedProcess(command, 0) if command[0] == 'cargo'
                else real(command, **kwargs))
            dev.test(['tests::one'])
            self.assertEqual(run.call_args.kwargs['env']['RUST_TEST_THREADS'], '5')
            self.assertFalse(dev.passed('test', {}))
            dev.test([])
            self.assertTrue(dev.passed('test', {}))
        with patch.object(dev, 'build_environment', return_value={'RUST_TEST_THREADS': '3'}), \
             patch.object(dev.pin, 'check_model'), \
             patch.object(dev.subprocess, 'run') as run:
            dev.test(['tests::one'])
            self.assertEqual(run.call_args.kwargs['env']['RUST_TEST_THREADS'], '3')

    def test_thread_budget_defaults_to_sixteen_cores_and_validates_overrides(self):
        clean = {k: v for k, v in os.environ.items() if k != 'ORACLE_GRAMMAR_THREADS'}
        with patch.dict(os.environ, clean, clear=True), patch.object(dev.os, 'cpu_count', return_value=88):
            self.assertEqual(dev.threads(), 16)
            with patch.object(dev.os, 'cpu_count', return_value=4):
                self.assertEqual(dev.threads(), 4)
            with patch.dict(os.environ, {'ORACLE_GRAMMAR_THREADS': '24'}):
                self.assertEqual(dev.threads(), 24)
            for value in ('0', 'many', '-2'):
                with self.subTest(value=value), patch.dict(os.environ, {'ORACLE_GRAMMAR_THREADS': value}), \
                     self.assertRaises(ValueError):
                    dev.threads()

    def test_check_reuses_passed_gates_and_reports_every_failure(self):
        launched = []

        class Gate:
            def __init__(self, name, command, environment=None):
                launched.append(name)
                self.name = name
                (dev.WORK/'check').mkdir(exist_ok=True)
                (dev.WORK/'check'/f'{name}.log').write_text(f'{name} output')

            def wait(self):
                return self.name, 'FAILED' if self.name in ('hygiene', 'clippy') else 'passed', 0.0

        (self.work/'build.json').write_text(json.dumps({'binaries': {'family-check': 'x'}}))
        with patch.object(dev, 'Gate', Gate), patch.object(dev.pin, 'check_model'), \
             patch.object(dev, 'build_environment', return_value={}), \
             patch.object(dev, 'passed', side_effect=lambda gate, extra: gate == 'test'), \
             patch('builtins.print') as shown, \
             self.assertRaisesRegex(ValueError, '2 gates failed: hygiene, clippy'):
            dev.check()
        self.assertEqual(sorted(launched), ['build', 'clippy', 'families', 'fmt', 'hygiene', 'pin', 'tools'])
        summary = [call.args[0] for call in shown.call_args_list]
        self.assertTrue(any(line.startswith('test') and 'reused' in line for line in summary))
        with patch.object(dev, 'Gate', Gate), patch.object(dev.pin, 'check_model'), \
             patch.object(dev, 'build_environment', return_value={}), \
             patch.object(dev, 'passed', return_value=True), patch('builtins.print'), \
             self.assertRaises(ValueError):
            launched.clear()
            dev.check(force=True)
        self.assertIn('test', launched)
        self.assertIn('families', launched)

if __name__ == '__main__':
    unittest.main()
