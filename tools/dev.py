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
import time
import tomllib

import pin

ROOT = Path(__file__).resolve().parent.parent
WORK = ROOT/'.cache/dev'
BINS = ('family-check', 'probe', 'inspect', 'score', 'validate-readings')


def source(root=ROOT):
    paths = sorted((root/'src').rglob('*.rs'))
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
    ready(binary)
    if action == 'families':
        if args:
            raise ValueError('families takes no arguments')
        args = [*json.loads((ROOT/'tests/families.json').read_text()), '--compositional']
    environment = {**os.environ, 'ORACLE_GRAMMAR_DIR': str(ROOT/'grammar')}
    overrides = [] if action == 'validate-readings' else modules()
    subprocess.run([str(WORK/binary), *args, *overrides], cwd=ROOT, env=environment, check=True,
                   stdout=stdout)



def test(args):
    pin.check_model()
    command = ['cargo', 'test', '--locked', '--release', '--lib', '--features', 'cli', *args]
    subprocess.run(command, cwd=ROOT, env=build_environment(), check=True)


def main():
    args = sys.argv[1:]
    if args in (['build'], ['build', '--optimized']):
        with lock():
            build('--optimized' in args)
    elif args and args[0] == 'test':
        test(args[1:])
    elif args and args[0] in ('family', 'families', 'probe', 'inspect', 'score', 'validate-readings'):
        with lock(shared=True):
            run(args[0], args[1:])
    else:
        raise ValueError('usage: dev.py build [--optimized] | test [filter] | family <file>... [--compositional] | families | probe <name> <text> | inspect <request> | score ... | validate-readings ...')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
