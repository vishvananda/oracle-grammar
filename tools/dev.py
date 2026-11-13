#!/usr/bin/env python3
"""Build helpers once; run hot-loaded phrase families, probes and inspections."""
from contextlib import contextmanager
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time
import tomllib

import pin

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT/'.cache/dev'
BINS = ('family-check', 'probe', 'inspect', 'score', 'validate-readings', 'rule-audit')
# Library tests and families cannot read documentation, CI definitions or the other tools.
NOT_GATE_INPUTS = ('docs/', '.github/', 'tools/')
GATE_TOOLS = ('tools/dev.py', 'tools/pin.py')


def source(root=ROOT):
    paths = sorted((root/'src').rglob('*.rs'))
    # Local model code and embedded wire/compatibility assets are compiled inputs.
    paths += sorted(p for p in (root/'crates').rglob('*')
                    if p.is_file() and p.suffix in ('.rs', '.toml', '.json', '.json5', '.txt')
                    and not {'target', '.cache'}.intersection(p.relative_to(root/'crates').parts))
    paths += [root/p for p in ('Cargo.toml', 'Cargo.lock', 'pins/model.json',
              '.cargo/config.toml', 'rust-toolchain.toml', 'clippy.toml',
              'grammar/lexical.ebnf', 'grammar/print-model.txt', 'grammar/print-weights.txt',
              'tools/dev.py', 'tools/pin.py')]
    digest = hashlib.sha256()
    for path in paths:
        digest.update(str(path.relative_to(root)).encode()+b'\0'+path.read_bytes())
    return digest.hexdigest()


@contextmanager
def lock(shared=False):
    WORK.mkdir(parents=True, exist_ok=True)
    with (WORK/'build.lock').open('a') as stream:
        fcntl.flock(stream, fcntl.LOCK_SH if shared else fcntl.LOCK_EX)
        yield


def compiler():
    environment = {k: v for k, v in os.environ.items()
                   if k != 'CARGO_TARGET_DIR' and (k in ('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC',
                            'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTUP_TOOLCHAIN',
                            'CARGO_INCREMENTAL', 'CARGO_BUILD_TARGET')
                   or k.startswith(('CARGO_PROFILE_', 'CARGO_TARGET_')))}
    versions = [subprocess.check_output(command, cwd=ROOT, text=True)
                for command in (['rustc', '-vV'], ['cargo', '-V'])]
    return {'environment': environment, 'versions': versions}



def build_environment(incremental=True):
    """Reuse the remembered cache; quick builds and tests recompile the frontend incrementally."""
    environment = dict(os.environ)
    if incremental:
        environment.setdefault('CARGO_INCREMENTAL', '1')
    config = tomllib.loads((ROOT/'.cargo/config.toml').read_text())
    if 'CARGO_TARGET_DIR' in environment or 'target-dir' in config.get('build', {}):
        return environment
    try:
        target = json.loads((WORK/'build.json').read_text()).get('target_directory')
    except (OSError, ValueError):
        return environment
    if target and Path(target).is_absolute():
        # A moved checkout must not silently start a cold dependency cache.
        if not Path(target).is_dir():
            raise ValueError(f'remembered Cargo target directory is missing: {target}; '
                             'set CARGO_TARGET_DIR to the cache to use')
        environment['CARGO_TARGET_DIR'] = target
    return environment


def build(optimized=False):
    pin.check_model()
    profile = 'optimized' if optimized else 'quick'
    try:
        cached = ready()
        if cached['profile'] == profile:
            print(json.dumps({**cached, 'reused': True}, indent=2))
            return
    except (ValueError, OSError):
        pass
    before = source()
    inputs = compiler()
    subprocess.run(['cargo', 'fmt', '--check'], cwd=ROOT, check=True)
    command = ['cargo', 'build', '--locked', '--release', '--features', 'cli',
               '--message-format=json']
    for name in BINS:
        command += ['--bin', name]
    if optimized:
        command += ['--config', 'profile.release.package.oracle-ir-grammar.opt-level=3']
    started = time.monotonic()
    environment = build_environment(incremental=not optimized)
    process = subprocess.Popen(command, cwd=ROOT, env=environment, text=True, stdout=subprocess.PIPE)
    paths = {}
    for line in process.stdout:
        event = json.loads(line)
        if event.get('reason') == 'compiler-message':
            print(event['message'].get('rendered', ''), end='')
        elif (event.get('reason') == 'compiler-artifact'
              and event['target']['name'] in BINS and event.get('executable')):
            paths[event['target']['name']] = Path(event['executable'])
    if process.wait():
        raise subprocess.CalledProcessError(process.returncode, command)
    if set(paths) != set(BINS):
        raise ValueError('Cargo did not report exactly the requested helper executables')
    if source() != before or compiler() != inputs:
        raise ValueError('compiled inputs changed during build; rebuild from frozen inputs')
    hashes = {}
    for name, path in paths.items():
        shutil.copy2(path, WORK/name)
        hashes[name] = pin.digest(WORK/name)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--locked', '--offline', '--no-deps', '--format-version=1'],
        cwd=ROOT, env=environment, text=True))
    manifest = {'source_sha256': before, 'compiler': inputs, 'binaries': hashes,
                'target_directory': metadata['target_directory'],
                'profile': profile,
                'build_seconds': time.monotonic()-started}
    pending = WORK/'build.json.next'
    pending.write_text(json.dumps(manifest, indent=2)+'\n')
    pending.replace(WORK/'build.json')
    print(json.dumps(manifest, indent=2))


def ready(binary=None):
    pin.check_model()
    try:
        manifest = json.loads((WORK/'build.json').read_text())
    except FileNotFoundError as error:
        raise ValueError('run python3 tools/dev.py build first') from error
    if manifest['source_sha256'] != source() or manifest['compiler'] != compiler():
        raise ValueError('compiled inputs changed; run python3 tools/dev.py build')
    if set(manifest['binaries']) != set(BINS):
        raise ValueError('helper manifest is incomplete; rebuild')
    selected = (binary,) if binary else BINS
    if any(pin.digest(WORK/name) != manifest['binaries'][name] for name in selected):
        raise ValueError('helper executable changed; rebuild')
    return manifest



def modules(root=ROOT):
    current = {p.stem: p for p in (root/'grammar/syntax').glob('*.ebnf')}
    tail = root/'grammar/tail/mined.ebnf'
    if tail.is_file():
        current['tail'] = tail
    declared = re.findall(r'\(\s*"([^"\n]+)",\s*"(?:syntax|tail)/[^"\n]+\.ebnf"\s*,?\s*\)',
                          (root/'src/engine.rs').read_text())
    return [f'--module={name}={current.get(name, os.devnull)}'
            for name in sorted(set(declared) | current.keys())]


def run(action, args, stdout=None):
    binary = {'family': 'family-check', 'families': 'family-check'}.get(action, action)
    manifest = ready(binary)
    environment = {**os.environ, 'ORACLE_GRAMMAR_DIR': str(ROOT/'grammar')}
    overrides = [] if action == 'validate-readings' else modules()
    if action != 'families':
        subprocess.run([str(WORK/binary), *args, *overrides], cwd=ROOT, env=environment,
                       check=True, stdout=stdout)
        return
    if args:
        raise ValueError('families takes no arguments')
    families = [*json.loads((ROOT/'tests/families.json').read_text()), '--compositional']
    environment['ORACLE_GRAMMAR_THREADS'] = str(threads())
    with receipt('families', {'helpers': manifest['binaries']}):
        subprocess.run([str(WORK/binary), *families, *overrides], cwd=ROOT, env=environment,
                       check=True, stdout=stdout)


def threads():
    """The verification budget: `ORACLE_GRAMMAR_THREADS`, else up to 16 of this host's cores."""
    value = os.environ.get('ORACLE_GRAMMAR_THREADS')
    if value is None:
        return min(16, os.cpu_count() or 1)
    if not value.isdigit() or int(value) < 1:
        raise ValueError('ORACLE_GRAMMAR_THREADS must be a positive integer')
    return int(value)


def tree(root=None):
    """Digest of every checked-out file a library test or family can read; None outside git."""
    root = ROOT if root is None else root
    try:
        listed = subprocess.run(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'],
                                cwd=root, capture_output=True, check=True).stdout.decode()
    except (OSError, subprocess.CalledProcessError, UnicodeDecodeError):
        return None
    digest = hashlib.sha256()
    for name in sorted(set(listed.split('\0')) - {''}):
        if name.endswith('.md') or (name.startswith(NOT_GATE_INPUTS) and name not in GATE_TOOLS):
            continue
        path = root/name
        if path.is_file():
            data = path.read_bytes()
            digest.update(f'{name}\0{len(data)}\0'.encode()+data)
    return digest.hexdigest()


def receipt_key(gate, digest, extra):
    toolchain = compiler()
    # Incremental compilation changes how a build is cached, not what it produces.
    toolchain['environment'].pop('CARGO_INCREMENTAL', None)
    identity = {'gate': gate, 'tree': digest, 'compiler': toolchain,
                'grammar': os.environ.get('ORACLE_GRAMMAR_DIR'), **extra}
    return hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()


def receipts():
    try:
        return json.loads((WORK/'receipts.json').read_text())
    except (OSError, ValueError):
        return {}


def passed(gate, extra):
    """Whether `gate` already passed on exactly the current files and compiler."""
    digest = tree()
    return digest is not None and receipts().get(gate) == receipt_key(gate, digest, extra)


@contextmanager
def receipt(gate, extra):
    """Record that `gate` passed, unless its inputs changed while it ran."""
    before = tree()
    yield
    if before is None or tree() != before:
        print(f'{gate}: files changed while it ran, so no receipt was recorded', file=sys.stderr)
        return
    # Standalone `test` need not have run a helper build to create this directory.
    WORK.mkdir(parents=True, exist_ok=True)
    # Its own lock: a gate may already hold the build lock shared.
    with (WORK/'receipts.lock').open('a') as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        current = receipts()
        current[gate] = receipt_key(gate, before, extra)
        pending = WORK/'receipts.json.next'
        pending.write_text(json.dumps(current, indent=2, sort_keys=True)+'\n')
        pending.replace(WORK/'receipts.json')


def test(args):
    pin.check_model()
    command = ['cargo', 'test', '--locked', '--release', '--workspace', '--lib', '--features', 'cli', *args]
    environment = build_environment()
    environment.setdefault('RUST_TEST_THREADS', str(threads()))
    if args:
        subprocess.run(command, cwd=ROOT, env=environment, check=True)
        return
    with receipt('test', {}):
        subprocess.run(command, cwd=ROOT, env=environment, check=True)


# Independent fast gates run alongside the Cargo chain; Cargo serializes on its build lock.
FAST_GATES = (
    ('fmt', ['cargo', 'fmt', '--all', '--check']),
    ('pin', [sys.executable, 'tools/pin.py', 'check']),
    ('hygiene', [sys.executable, 'tools/hygiene.py']),
    ('tools', [sys.executable, '-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_*.py']),
)
CLIPPY = ['cargo', 'clippy', '--locked', '--workspace', '--all-targets', '--all-features',
          '--', '-D', 'warnings']


class Gate:
    def __init__(self, name, command, environment=None):
        self.name, self.log = name, WORK/'check'/f'{name}.log'
        self.started = time.monotonic()
        with self.log.open('w') as stream:
            process = subprocess.Popen(command, cwd=ROOT, env=environment, stdout=stream,
                                       stderr=subprocess.STDOUT)
        # Time each gate to its own exit, not to when the summary collects it.
        self.waiter = threading.Thread(target=self.reap, args=(process,), daemon=True)
        self.waiter.start()

    def reap(self, process):
        self.code = process.wait()
        self.seconds = time.monotonic()-self.started

    def wait(self):
        self.waiter.join()
        return self.name, 'passed' if self.code == 0 else 'FAILED', self.seconds


def check(force=False):
    """Every contribution gate, sharing one build cache; unchanged passed gates are reused."""
    pin.check_model()
    (WORK/'check').mkdir(parents=True, exist_ok=True)
    # Gates that are dev.py commands choose their own Cargo environment. Passing this
    # one's incremental setting would change the helpers' recorded compiler identity.
    environment = {**os.environ, 'ORACLE_GRAMMAR_THREADS': str(threads())}
    tool = [sys.executable, str(ROOT/'tools/dev.py')]
    fast = [Gate(name, command) for name, command in FAST_GATES]
    clippy = {**build_environment(), 'ORACLE_GRAMMAR_THREADS': environment['ORACLE_GRAMMAR_THREADS']}
    results = [Gate('clippy', CLIPPY, clippy).wait()]
    built = Gate('build', [*tool, 'build'], environment).wait()
    results.append(built)
    slow = []
    if built[1] == 'passed':
        helpers = json.loads((WORK/'build.json').read_text())['binaries']
        if not force and passed('families', {'helpers': helpers}):
            results.append(('families', 'reused', 0.0))
        else:
            slow.append(Gate('families', [*tool, 'families'], environment))
    else:
        results.append(('families', 'skipped', 0.0))
    if not force and passed('test', {}):
        results.append(('test', 'reused', 0.0))
    else:
        slow.append(Gate('test', [*tool, 'test'], environment))
    results += [gate.wait() for gate in [*fast, *slow]]
    order = [name for name, _ in FAST_GATES] + ['clippy', 'build', 'families', 'test']
    failed = []
    for name, status, seconds in sorted(results, key=lambda result: order.index(result[0])):
        print(f'{name:9} {status:7} {seconds:6.1f} s')
        if status == 'FAILED':
            failed.append(name)
    for name in failed:
        log = WORK/'check'/f'{name}.log'
        print(f'\n== {name} ({log})\n' + '\n'.join(log.read_text().splitlines()[-30:]))
    if failed:
        raise ValueError(f'{len(failed)} gates failed: {", ".join(failed)}')


def main():
    args = sys.argv[1:]
    if args in (['build'], ['build', '--optimized']):
        with lock():
            build('--optimized' in args)
    elif args in (['check'], ['check', '--force']):
        check('--force' in args)
    elif args and args[0] == 'test':
        test(args[1:])
    elif args and args[0] in ('family', 'families', 'probe', 'inspect', 'score', 'validate-readings'):
        with lock(shared=True):
            run(args[0], args[1:])
    else:
        raise ValueError('usage: dev.py build [--optimized] | check [--force] | test [filter] | family <file>... [--compositional] | families | probe <name> <text> | inspect <request> | score ... | validate-readings ...')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
