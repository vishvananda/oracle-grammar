#!/usr/bin/env python3
"""Verify and run one native author helper under the shared CPU budget."""
from contextlib import nullcontext
import fcntl
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True

# Installed as support/bin/{family-check,probe,inspect}; imports are frozen beside it.
support = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(support/'runner'))
import retire as r
import retire_cpu

NAMES = ('family-check', 'probe', 'inspect')


def inherited(pool, count):
    value = os.environ.get('RETIRE_CPU_LEASE_FDS', '')
    if not value:
        return ()
    try:
        fds = tuple(int(part) for part in value.split(','))
        if len(fds) != count or len(set(fds)) != count:
            raise ValueError('incorrect inherited token count')
        inodes = set()
        for fd in fds:
            target = Path(os.readlink(f'/proc/self/fd/{fd}'))
            if target.parent != pool or not target.name.startswith('token-') or not target.name.endswith('.lock'):
                raise ValueError('inherited descriptor is not a pool token')
            info = os.fstat(fd)
            if (info.st_dev, info.st_ino) != (target.stat().st_dev, target.stat().st_ino):
                raise ValueError('inherited descriptor does not match token path')
            inodes.add((info.st_dev, info.st_ino))
            # Shared open-file-description locks remain held when the parent closes.
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if len(inodes) != count:
            raise ValueError('repeated inherited token')
        return fds
    except (OSError, ValueError) as exc:
        raise ValueError('invalid inherited CPU lease') from exc


def main():
    name = Path(sys.argv[0]).name
    r.require(name in NAMES, 'unrecognized helper wrapper')
    run = support.parent
    tasks = list((run/'tasks').glob('*.json'))
    r.require(len(tasks) == 1, 'expected one immutable task descriptor')
    task = r.read(tasks[0])
    manifest = r.read(run/'support.json')
    r.require(manifest['tasks'][tasks[0].stem] == r.digest(tasks[0]), 'task descriptor changed')
    native = support/'native'/name
    r.require(r.digest(native) == manifest['files']['native/'+name], 'native helper changed')
    r.require(r.digest(Path(__file__)) == manifest['files']['bin/'+name], 'helper wrapper changed')
    count = task.get('helper_threads', 2)
    pool = Path(task['cpu_pool']).resolve()
    fds = inherited(pool, count)
    context = nullcontext(fds) if fds else retire_cpu.reserve(pool, count, task.get('cpu_capacity', 32))
    with context as lease:
        environment = dict(os.environ)
        environment['ORACLE_GRAMMAR_THREADS'] = str(count)
        # A nested native process can retain the lease if this wrapper is killed.
        extra = os.environ.get('RETIRE_HELPER_EXTRA_FDS', '') if fds else ''
        inherited_fds = set(lease) | ({int(fd) for fd in extra.split(',')} if extra else set())
        return subprocess.run([str(native), *sys.argv[1:]], env=environment,
                              pass_fds=tuple(sorted(inherited_fds)), check=False).returncode


if __name__ == '__main__':
    sys.exit(main())
