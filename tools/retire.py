#!/usr/bin/env python3
"""Frozen, deletion-only retirement jobs. Workers never edit a source checkout."""
import argparse
from contextlib import contextmanager
from collections import defaultdict
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

# Imports from a frozen runner must not add bytecode files to its input bundle.
sys.dont_write_bytecode = True
import dev
import mined

HELPERS = ('score', 'validate-readings', 'rule-audit')
TOOLING = ('retire.py', 'retire_worker.py', 'mined.py', 'dev.py', 'pin.py')
PRINT_GATES = ('verified', 'exact', 'default', 'exact_default')
_LEASES = []


def read(path):
    return json.loads(Path(path).read_text())


def rows(path):
    with Path(path).open() as stream:
        return [json.loads(line) for line in stream]


def write(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    temporary.write_text(json.dumps(value, indent=2, ensure_ascii=False) + '\n')
    os.replace(temporary, path)


def jsonl(path, values):
    with Path(path).open('w') as stream:
        for value in values:
            stream.write(json.dumps(value, ensure_ascii=False) + '\n')


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def tree(path):
    return {str(p.relative_to(path)): digest(p) for p in sorted(Path(path).rglob('*'))
            if p.is_file()}


@contextmanager
def locked(path, blocking=True):
    """Kernel-owned leases: a crashed worker cannot keep a job locked."""
    with Path(path).open('a') as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
        except BlockingIOError:
            yield False
            return
        try:
            _LEASES.append(stream.fileno())
            yield True
        finally:
            _LEASES.remove(stream.fileno())
            # Close rather than LOCK_UN: an inherited helper descriptor must
            # retain the lease if its Python worker exits before that helper.


def flags(row):
    if 'printing' in row:
        details, matching = row['printing'], row.get('matching_readings', [])
        require(matching and len(set(matching)) == len(matching)
                and len(details) == len(matching)
                and {d.get('reading') for d in details} == set(matching),
                'per-reading print coverage is missing, duplicated or incomplete')
        values = [flags(detail) for detail in details]
        return [all(value[i] for value in values) for i in range(len(PRINT_GATES))]
    require('matching_readings' not in row, 'matching readings lack printing reports')
    exact = row.get('exact') is True
    default = isinstance(row.get('default'), str) or exact and 'default' not in row
    trip = row.get('round_trip', [0, 0])
    return [bool(default or exact or trip[1] > 0), exact, bool(default),
            exact and 'default' not in row]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def verify(run):
    manifest = read(run/'manifest.json')
    require(manifest['format'] == 3, 'unsupported manifest; use the campaign frozen runner')
    require(manifest['tooling'] == tooling(), 'workflow implementation changed; freeze a new run')
    actual = tree(run/'frozen')
    require(actual == manifest['files'], 'frozen input changed or a file was added/removed')
    return manifest


def tooling():
    return {name: digest(Path(__file__).parent/name) for name in TOOLING}


def freeze(args):
    run, source = args.run.resolve(), args.source.resolve()
    runner_identity = tooling()
    require(not run.exists(), 'run directory already exists')
    require(1 <= args.slots <= 16 and 1 <= args.threads <= 32
            and args.slots * args.threads <= 32, 'worker CPU budget must be at most 32')
    require(not subprocess.check_output(['git', 'status', '--porcelain'], cwd=source),
            'freeze from a clean source checkout')
    before = dev.source(source)
    build = read(args.helpers/'build.json')
    require(build['source_sha256'] == before, 'helpers do not match compiled source')
    for name in HELPERS:
        require(digest(args.helpers/name) == build['binaries'][name],
                'helper checksum mismatch: ' + name)
    allowed = read(args.allowed)['cards']
    require(allowed and len(allowed) == len(set(allowed)), 'allowed faces must be unique/nonempty')
    allowed = set(allowed)
    gold = rows(args.gold)
    names = [f['name'] for record in gold for f in record['faces']]
    require(set(names) == set(allowed), 'gold must contain exactly the explicit allowed population')
    oracle = read(args.oracle)
    require({f['name'] for record in oracle for f in record['cards']} == set(allowed),
            'Oracle input must contain exactly the allowed population')
    baseline = rows(args.parse)
    require([r['name'] for r in baseline] == names
            and all(r['status'] == 'equal' for r in baseline), 'baseline parse is incomplete/unequal')
    index, occurrences = {}, defaultdict(list)
    addresses = [(record, face) for record, value in enumerate(gold)
                 for face in range(len(value['faces']))]
    for ordinal, (row, (record, face)) in enumerate(zip(baseline, addresses, strict=True)):
        entry = {'name': row['name'], 'text': row['text'], 'parse': row,
                 'record': record, 'face': face}
        index[str(ordinal)] = entry
        occurrences[row['name'], row['text']].append(entry)
    for path in args.prints:
        groups = defaultdict(list)
        for row in rows(path):
            if row['name'] in allowed:
                groups[row['name'], row['text']].append(row)
        for key, reports in groups.items():
            entries = occurrences[key]
            require(entries and len(entries) == len(reports),
                    'print overlay occurrence count/text differs: ' + key[0])
            for entry, row in zip(entries, reports, strict=True):
                entry['flags'] = flags(row)
    require(all('flags' in entry for entry in index.values()), 'print baseline is incomplete')
    # Prepare privately, then publish one complete immutable input bundle.
    temporary = run.with_name(run.name + '.' + uuid.uuid4().hex + '.tmp')
    frozen = temporary/'frozen'
    frozen.mkdir(parents=True)
    try:
        shutil.copytree(source/'grammar', frozen/'grammar')
        (frozen/'runner').mkdir()
        for name in TOOLING:
            shutil.copy2(Path(__file__).parent/name, frozen/'runner'/name)
        (frozen/'bin').mkdir()
        for name in HELPERS:
            shutil.copy2(args.helpers/name, frozen/'bin'/name)
        shutil.copy2(args.helpers/'build.json', frozen/'build.json')
        jsonl(frozen/'gold.jsonl', gold)
        write(frozen/'oracle.json', oracle)
        write(frozen/'allowed.json', {'cards': sorted(allowed)})
        jsonl(frozen/'parse.jsonl', baseline)
        write(frozen/'index.json', index)
        require(before == dev.source(source) and tree(source/'grammar') == tree(frozen/'grammar'),
                'source changed while freezing')
        require(runner_identity == tooling() == tree(frozen/'runner'),
                'workflow implementation changed while freezing')
        for directory in ('jobs', 'slots'):
            (temporary/directory).mkdir()
        manifest = {'format': 3, 'source': before, 'commit': subprocess.check_output(
            ['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip(),
            'population': len(baseline), 'names': len(allowed),
            'slots': args.slots, 'threads': args.threads,
            'files': tree(frozen), 'tooling': runner_identity}
        write(temporary/'manifest.json', manifest)
        os.rename(temporary, run)
    finally:
        if temporary.exists():
            shutil.rmtree(temporary)
    print(json.dumps({'run': str(run), 'population': len(baseline), 'names': len(allowed),
                      'source': before, 'runner': str(run/'frozen/runner/retire.py')}))


def execute(run, directory, grammar, program, arguments, threads):
    """Run only an explicitly frozen helper, with no shell or build invocation."""
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith('ORACLE_GRAMMAR_')}
    environment.update(ORACLE_GRAMMAR_DIR=str(grammar.resolve()),
                       ORACLE_GRAMMAR_THREADS=str(threads))
    command = [str((run/'frozen/bin'/program).resolve()), *map(str, arguments)]
    started, wall, code = time.time(), time.perf_counter(), None
    try:
        with (directory/(program + '.log')).open('w') as log:
            completed = subprocess.run(command, cwd=directory, env=environment, stdout=log,
                                       stderr=log, check=True, pass_fds=tuple(_LEASES))
            code = completed.returncode
    except subprocess.CalledProcessError as error:
        code = error.returncode
        raise
    finally:
        with (directory/'commands.jsonl').open('a') as stream:
            stream.write(json.dumps({'program': program, 'started_unix': started,
                                     'wall_seconds': time.perf_counter() - wall,
                                     'returncode': code}) + '\n')


def scan(args):
    run = args.run.resolve()
    manifest = verify(run)
    with locked(run/'coordinator.lock'):
        require(not (run/'scan').exists(), 'scan already exists; use a new frozen run')
        directory = run/'scan'
        directory.mkdir()
        frozen = run/'frozen'
        execute(run, directory, frozen/'grammar', 'rule-audit',
                [frozen/'gold.jsonl', frozen/'oracle.json', frozen/'allowed.json',
                 directory/'rules.json'], manifest['slots'] * manifest['threads'])
        verify(run)
        report = read(directory/'rules.json')
        require(report['summary']['faces'] == manifest['population']
                and report['summary']['equal'] == manifest['population']
                and report['summary']['at_risk'] == 0, 'rule audit is incomplete or at risk')
        candidates = report['unused']
        if args.rules:
            require(set(args.rules) <= set(candidates), 'requested rules are not audited-unused')
            candidates = sorted(set(args.rules))
        for rule in candidates:
            require(mined.SEEDED.fullmatch(rule), 'invalid candidate name')
            identity = hashlib.sha256(rule.encode()).hexdigest()[:20]
            job = run/'jobs'/identity
            job.mkdir()
            write(job/'job.json', {'rule': rule, 'scan': digest(directory/'rules.json'),
                                  'snapshot': digest(run/'manifest.json')})
        write(run/'scan-complete.json', {'report': digest(directory/'rules.json')})
        print(json.dumps({'candidates': len(candidates), 'summary': report['summary']}))


def assemble(args):
    """Combine rule names, never overlapping worker patches or source files."""
    run = args.run.resolve()
    verify(run)
    with locked(run/'coordinator.lock'):
        require(not (run/'combined').exists(), 'combined candidate already exists')
        selected = []
        for job in sorted((run/'jobs').iterdir()):
            with locked(job/'lease', blocking=False) as acquired:
                require(acquired, 'a worker is still running')
                if not (job/'result.json').exists():
                    continue
                result = read(job/'result.json')
                if result['status'] == 'provisional-pass':
                    import retire_worker
                    retire_worker.job_inputs(run, job)
                    candidate = retire_worker.attempt_path(job, result)
                    require(result['snapshot'] == digest(run/'manifest.json')
                            and result['grammar'] == tree(candidate/'grammar'),
                            'worker candidate changed')
                    selected.append(read(job/'job.json')['rule'])
        require(selected, 'no provisional passes to assemble')
        combined = run/'combined'
        combined.mkdir()
        shutil.copytree(run/'frozen/grammar', combined/'grammar')
        removed, kept = mined.remove(combined, selected)
        require(removed == sorted(selected) and not kept, 'combined removal has kept dependencies')
        write(combined/'selection.json', {'rules': removed, 'grammar': tree(combined/'grammar'),
                                         'snapshot': digest(run/'manifest.json')})
        # Produce a reviewable Git patch, without writing into any checkout.
        import difflib
        patch = []
        for relative in tree(run/'frozen/grammar'):
            before = (run/'frozen/grammar'/relative).read_text().splitlines(keepends=True)
            after = (combined/'grammar'/relative).read_text().splitlines(keepends=True)
            patch.extend(difflib.unified_diff(before, after, 'a/grammar/'+relative,
                                             'b/grammar/'+relative))
        (combined/'retirement.patch').write_text(''.join(patch))
        print(json.dumps({'rules': removed, 'patch': str(combined/'retirement.patch'),
                          'status': 'requires-full-validation'}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    f = commands.add_parser('freeze')
    f.add_argument('run', type=Path)
    for name in ('source', 'helpers', 'gold', 'oracle', 'allowed', 'parse'):
        f.add_argument('--'+name, required=True, type=Path)
    f.add_argument('--prints', nargs='+', required=True, type=Path,
                   help='oldest to newest complete print ledger overlays')
    f.add_argument('--slots', type=int, default=2)
    f.add_argument('--threads', type=int, default=2)
    for name in ('scan', 'assemble', 'validate', 'worker', 'status', 'report'):
        command = commands.add_parser(name)
        command.add_argument('run', type=Path)
        if name == 'scan':
            command.add_argument('--rules', nargs='+', help='only queue these audited-unused rules')
        if name == 'worker':
            command.add_argument('--name', required=True)
            command.add_argument('--limit', type=int, default=1)
            command.add_argument('--rules', nargs='+', help='claim only these rules for a paired trial')
        if name == 'report':
            command.add_argument('--rules', nargs='+', help='report only the assigned rule bundle')
    args = parser.parse_args()
    if args.command in ('freeze', 'scan', 'assemble'):
        globals()[args.command](args)
    else:
        import retire_worker
        getattr(retire_worker, args.command)(args)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(str(error))
