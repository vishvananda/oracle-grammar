"""Protect the fast loop: syntax/reference changes do not invalidate binaries."""
from pathlib import Path
import shutil
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


if __name__ == '__main__':
    unittest.main()
