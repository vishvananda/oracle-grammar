#!/usr/bin/env python3
"""Run a candidate checkout's eight gates with the candidate grammar bound."""
import argparse
import importlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True


def load_dev(source):
    """Import only from the candidate in this fresh, dedicated process."""
    tools = source/'tools'
    if not (tools/'dev.py').is_file() or not (tools/'pin.py').is_file():
        raise ValueError('candidate checkout does not contain repository gate tooling')
    if 'dev' in sys.modules or 'pin' in sys.modules:
        raise ValueError('candidate gate driver must run in a fresh process')
    sys.path.insert(0, str(tools))
    dev = importlib.import_module('dev')
    if Path(dev.__file__).resolve() != (tools/'dev.py').resolve() or dev.ROOT != source:
        raise ValueError('repository gate driver imported from a different checkout')
    return dev


def lease_descriptors(value):
    """Validate unique inherited, open descriptors before starting any gates."""
    if isinstance(value, str):
        if not value:
            return ()
        if any(not token.isascii() or not token.isdigit() for token in value.split(',')):
            raise ValueError('lease descriptors must be comma-separated integers')
        value = tuple(int(token) for token in value.split(','))
    if any(type(fd) is not int or fd < 3 for fd in value) or len(set(value)) != len(value):
        raise ValueError('lease descriptors must be unique open file descriptors >= 3')
    for fd in value:
        try:
            os.fstat(fd)
        except OSError as error:
            raise ValueError(f'lease descriptor {fd} is not open') from error
    return tuple(value)


def inherited_gate_factory(dev, descriptors):
    """Keep Gate's wait/reap behavior while passing leases to each gate process.

    Grandchildren depend on their gate parent remaining alive; this does not
    guarantee leases for an arbitrary orphaned descendant process.
    """
    class InheritedGate(dev.Gate):
        def __init__(self, name, command, environment=None):
            self.name, self.log = name, dev.WORK/'check'/f'{name}.log'
            self.started = time.monotonic()
            with self.log.open('w') as stream:
                process = subprocess.Popen(command, cwd=dev.ROOT, env=environment,
                                           stdout=stream, stderr=subprocess.STDOUT,
                                           pass_fds=descriptors)
            self.waiter = threading.Thread(target=self.reap, args=(process,), daemon=True)
            self.waiter.start()
    return InheritedGate


def check(source, force=False, dev=None, lease_fds=()):
    """Reuse the candidate's gate commands and receipts, with explicit environments."""
    source = Path(source).resolve(strict=True)
    if not source.is_dir() or not (source/'grammar').is_dir():
        raise ValueError('candidate checkout or grammar is missing')
    if dev is None:
        dev = load_dev(source)
    if dev.ROOT != source:
        raise ValueError('repository gate source does not match candidate')
    gate = inherited_gate_factory(dev, lease_descriptors(lease_fds))
    clean = {key: value for key, value in os.environ.items() if not key.startswith('ORACLE_GRAMMAR_')}
    threads = str(dev.threads())
    native = {**clean, 'ORACLE_GRAMMAR_DIR': str(source/'grammar'), 'ORACLE_GRAMMAR_THREADS': threads}
    original = dict(os.environ)
    try:
        # Receipt identities are computed in this process as well as in its children.
        os.environ.clear()
        os.environ.update(native)
        dev.pin.check_model()
        (dev.WORK/'check').mkdir(parents=True, exist_ok=True)
        tool = [sys.executable, str(source/'tools/dev.py')]
        fast = [gate(name, command, clean) for name, command in dev.FAST_GATES]
        clippy = {**dev.build_environment(), 'ORACLE_GRAMMAR_DIR': native['ORACLE_GRAMMAR_DIR'],
                  'ORACLE_GRAMMAR_THREADS': threads}
        results = [gate('clippy', dev.CLIPPY, clippy).wait()]
        built = gate('build', [*tool, 'build'], native).wait()
        results.append(built)
        slow = []
        if built[1] == 'passed':
            helpers = json.loads((dev.WORK/'build.json').read_text())['binaries']
            if not force and dev.passed('families', {'helpers': helpers}):
                results.append(('families', 'reused', 0.0))
            else:
                slow.append(gate('families', [*tool, 'families'], native))
        else:
            results.append(('families', 'skipped', 0.0))
        if not force and dev.passed('test', {}):
            results.append(('test', 'reused', 0.0))
        else:
            slow.append(gate('test', [*tool, 'test'], native))
        results += [gate.wait() for gate in [*fast, *slow]]
        order = [name for name, _ in dev.FAST_GATES] + ['clippy', 'build', 'families', 'test']
        failed = []
        for name, status, seconds in sorted(results, key=lambda row: order.index(row[0])):
            print(f'{name:9} {status:7} {seconds:6.1f} s')
            if status == 'FAILED':
                failed.append(name)
        for name in failed:
            log = dev.WORK/'check'/f'{name}.log'
            print(f'\n== {name} ({log})\n' + '\n'.join(log.read_text().splitlines()[-30:]))
        if failed:
            raise ValueError(f'{len(failed)} gates failed: {", ".join(failed)}')
        return results
    finally:
        os.environ.clear()
        os.environ.update(original)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('--force', action='store_true')
    parser.add_argument('--lease-fds', default='', help='comma-separated inherited lease descriptors')
    args = parser.parse_args()
    check(args.source, force=args.force, lease_fds=args.lease_fds)


if __name__ == '__main__':
    main()
