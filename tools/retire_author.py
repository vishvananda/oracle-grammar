#!/usr/bin/env python3
"""Bounded diagnostics for isolated syntax-authoring trials; never accept source."""
import argparse
import copy
from contextlib import nullcontext
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

sys.dont_write_bytecode = True
import dev
import mined
import retire as r
import retire_cohort as cohort
import retire_screen_packet as screen_packet
import retire_worker as w


def seeded(root):
    return {name for path in mined.grammar_files(root)
            for name in mined.definitions(path.read_text().splitlines())
            if mined.SEEDED.fullmatch(name)}


def edit_leaf(value, path, replacement):
    r.require(path and isinstance(path, list), 'edit needs a nonempty path within abilities')
    current = value
    for key in path[:-1]:
        current = current[key]
    old = current[path[-1]]
    r.require(not isinstance(old, (dict, list)) and not isinstance(replacement, (dict, list))
              and old != replacement, 'edits replace a changed primitive operand, not a whole IR')
    r.require(not any(str(key) in ('slot', 'result', 'binding', 'id') for key in path),
              'identifier renaming is not an operand-edit test')
    current[path[-1]] = replacement


def changed_files(root):
    environment = dict(os.environ, GIT_OPTIONAL_LOCKS='0')
    tracked = subprocess.check_output(['git', 'diff', '--name-only'], cwd=root, text=True,
                                      env=environment).splitlines()
    untracked = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'],
                                        cwd=root, text=True, env=environment).splitlines()
    return sorted(set(tracked + untracked))


def regenerate_removals(base, root, task):
    """Remove assigned rules in a private copy while preserving authored syntax."""
    check_root_hooks(base, root)
    directory = root/'.cache'/('assigned-removal-'+uuid.uuid4().hex)
    directory.mkdir(parents=True)
    shutil.copytree(root/'grammar', directory/'grammar')
    # Seeded files are canonical outputs, even when a worker damaged their hooks.
    for name in ('syntax/mined.ebnf', 'tail/mined.ebnf'):
        source = base/'grammar'/name
        if source.exists():
            shutil.copy2(source, directory/'grammar'/name)
    removed, kept = mined.remove(directory, task['rules'])
    r.require(set(removed) == set(task['rules']) and not kept, 'assigned rules have retained users')
    before, after = r.tree(root/'grammar'), r.tree(directory/'grammar')
    r.require(before.keys() == after.keys(), 'removal unexpectedly changed grammar files')
    changed = [name for name in before if before[name] != after[name]]
    r.require(all('grammar/'+name in task['write_files'] for name in changed),
              'removal would change files outside the task write set')
    # Canonical removal must never bypass the protected typed-hook contract.
    check_root_hooks(base, directory)
    for name in changed:
        backup = directory/'backup'/name
        backup.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root/'grammar'/name, backup)
    for name in changed:
        target = root/'grammar'/name
        temporary = target.with_name(target.name+'.'+uuid.uuid4().hex+'.tmp')
        shutil.copy2(directory/'grammar'/name, temporary)
        os.replace(temporary, target)
    return {'removed': removed, 'backups': str(directory/'backup'),
            'status': 'mechanical edit only; run diagnostics before claiming a pass'}


def check_seeded_files(base, root, rules, directory):
    expected = directory/'expected-removals'
    shutil.copytree(base/'grammar', expected/'grammar')
    removed, kept = mined.remove(expected, rules)
    r.require(set(removed) == set(rules) and not kept, 'seeded removal has retained users')
    for name in ('syntax/mined.ebnf', 'tail/mined.ebnf'):
        reference = expected/'grammar'/name
        if reference.exists():
            clean = lambda text: [line.strip() for line in text.splitlines() if line.strip()]
            r.require(clean(reference.read_text()) == clean((root/'grammar'/name).read_text()),
                      'seeded file has non-removal edits; use --remove-assigned and ordinary hook requests')


EARLY_PARSE_FULL_OCCURRENCES = 128


def _cohort_index(index):
    by_name = {}
    for ordinal, row in index.items():
        by_name.setdefault(row['name'], []).append((int(ordinal), row))
    return by_name


def _primary_cohort(by_name, records, full_names, targets):
    primary = cohort.close_cohort(by_name, records, {'names': targets})
    r.require(set(primary['names']) <= set(full_names), 'primary closure escaped full diagnostic cohort')
    return primary


def early_parse_screen(run, directory, grammar, task, data=None, compact=None):
    """Reject obvious primary-cohort regressions; a pass is not acceptance."""
    targets = task.get('target_faces', [])
    if not targets:
        return None
    if compact is None:
        index, records, _ = data
        by_name = _cohort_index(index)
        full = cohort.close_cohort(by_name, records, {'names': task['cohort']}, already_closed=True)
        if full['closure_occurrence_count'] <= EARLY_PARSE_FULL_OCCURRENCES:
            return None
        primary = _primary_cohort(by_name, records, full['names'], targets)
    folder = directory/'parse-screen'
    folder.mkdir()
    if compact is None:
        selected = w.subset(folder, primary['names'], data)
        expected = [(int(ordinal), row) for ordinal, row in index.items() if row['name'] in selected]
        r.require(len(expected) == primary['closure_occurrence_count'], 'primary occurrence count changed')
        full_count = full['closure_occurrence_count']
    else:
        r.jsonl(folder/'gold.jsonl', compact['gold'])
        r.write(folder/'oracle.json', compact['oracle'])
        r.write(folder/'slice.json', {'cards': compact['names']})
        expected = [(row['ordinal'], {'record': row['record'], 'face': row['face'], 'name': row['name'],
                                      'text': row['text'], 'parse': {'readings': row['readings']}})
                    for row in compact['entries']]
        full_count = compact['full_occurrences']
    r.execute(run/'inputs', folder, grammar, 'score',
              [folder/'gold.jsonl', folder/'oracle.json', folder/'slice.json', folder/'rows.jsonl'], 2)
    rows = r.rows(folder/'rows.jsonl')
    r.require(len(rows) == len(expected), 'primary score omitted occurrences')
    failures = []
    for row, (ordinal, old) in zip(rows, expected, strict=True):
        if (row.get('name') != old['name'] or row.get('text') != old['text']
                or row.get('status') != 'equal' or row.get('readings') != old['parse']['readings']):
            failures.append({'ordinal': ordinal, 'record': old['record'], 'face_index': old['face'],
                             'face': old['name'], 'gate': 'parse/reading-count/text'})
    return {'status': 'failed' if failures else 'passed', 'primary_occurrences': len(expected),
            'full_occurrences': full_count, 'failures': failures,
            'scope': 'parse-only rejection screen; full native and print checks remain required'}


def apply_hooks(grammar, requests):
    r.require(isinstance(requests, list) and len(requests) <= 4, 'at most four ordinary hook requests')
    kinds = {'ability': 'Ability::', 'line': 'Ability::', 'event_clause': 'EventSpec::'}
    seen = set()
    for entry in requests:
        r.require(set(entry) == {'root', 'production'}, 'invalid hook request fields')
        root, production = entry['root'], entry['production']
        r.require(root in kinds and isinstance(production, str)
                  and mined.BARE.fullmatch(production) and not mined.SEEDED.fullmatch(production)
                  and (root, production) not in seen, 'invalid or repeated ordinary hook request')
        seen.add((root, production))
        definitions, declarations = [], []
        for path in mined.grammar_files(grammar.parent):
            lines = path.read_text().splitlines()
            found = mined.definitions(lines)
            if root in found:
                definitions.append((path, lines, found[root]))
            for _, line, owner, comments in mined.code(lines):
                if mined.DEFINITION.match(line) and owner == production:
                    declarations.append('\n'.join(lines[n] for n in comments))
        r.require(len(definitions) == len(declarations) == 1
                  and '@sem '+kinds[root] in declarations[0], 'hook needs one correctly typed declaration')
        path, lines, entries = definitions[0]
        names = mined.alternatives(entries)
        r.require(names is not None, 'ordinary hook root must be a bare alternative list')
        if production not in {name for group in names for name in group}:
            lines[entries[-1][0]] += ' | '+production
            path.write_text('\n'.join(lines)+'\n')
    return requests


ROOT_HOOKS = ('ability', 'line', 'event_clause')
# Preserve quoted text and identifier boundaries while ignoring layout whitespace.
ROOT_TOKEN = re.compile(r'''"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|[A-Za-z_]\w*|\d+|[^\s]''')


def root_declarations(root):
    """Return locations and whitespace-insensitive declarations of protected hooks."""
    found = {name: [] for name in ROOT_HOOKS}
    for path in mined.grammar_files(root):
        lines = path.read_text().splitlines()
        definitions = mined.definitions(lines)
        for index, line, owner, comments in mined.code(lines):
            if owner not in found or not mined.DEFINITION.match(line):
                continue
            prefix = '\n'.join(lines[n] for n in comments)
            sem = re.findall(r'@sem\s+(.*?)(?:\*/)', prefix, re.S)
            code = '\n'.join(line for _, line in definitions[owner])
            tokens = tuple(ROOT_TOKEN.findall(code))
            meanings = tuple(tuple(ROOT_TOKEN.findall(item)) for item in sem)
            found[owner].append((path.relative_to(root).as_posix(), tokens, meanings))
    return found


def check_root_hooks(base, root):
    r.require(root_declarations(base) == root_declarations(root),
              'direct root dispatch edits are forbidden; request typed hooks')


def verify(run, task_id):
    support = r.read(run/'support.json')
    r.require(r.tree(run/'support') == support['files'], 'trial tooling or helpers changed')
    manifest = r.verify(run/'inputs')
    descriptor = run/'tasks'/f'{task_id}.json'
    r.require(r.digest(descriptor) == support['tasks'][task_id], 'task descriptor changed')
    r.require(r.digest(Path(__file__)) == support['files'].get('runner/retire_author.py'),
              'use the campaign frozen authoring runner')
    task = r.read(descriptor)
    if 'packet' in task:
        packet = run/'packet.json'
        r.require(not packet.is_symlink() and Path(task['packet']).resolve() == packet.resolve()
                  and r.digest(packet) == support.get('packet_sha256'), 'task packet changed')
    if 'screen_packet_sha256' in task:
        screen_packet.load(run, task, r.digest(run/'inputs/manifest.json'), support)
    root = Path(task['checkout'])
    r.require(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
              == manifest['commit'], 'worker changed its base commit')
    r.require(dev.source(root) == manifest['source'], 'compiled-input change requires escalation')
    r.require(not subprocess.check_output(['git', 'diff', '--cached', '--name-only'], cwd=root),
              'workers cannot stage or commit')
    r.require(set(changed_files(root)) <= set(task['write_files']), 'worker exceeded its write set')
    check_root_hooks(run/'inputs/frozen', root)
    return task, root, manifest


def helper(run, folder, grammar, program, arguments):
    command = [str(run/'support/bin'/program), *map(str, arguments)]
    env = {k: v for k, v in os.environ.items() if not k.startswith('ORACLE_GRAMMAR_')}
    env.update(ORACLE_GRAMMAR_DIR=str(grammar), ORACLE_GRAMMAR_THREADS='2')
    # check() owns these descriptors; the wrapper verifies their token paths.
    cpu_fds = getattr(r, '_CPU_LEASES', ())
    if cpu_fds:
        env['RETIRE_CPU_LEASE_FDS'] = ','.join(map(str, cpu_fds))
        env['RETIRE_HELPER_EXTRA_FDS'] = ','.join(map(str, r._LEASES))
    else:
        env.pop('RETIRE_CPU_LEASE_FDS', None)
        env.pop('RETIRE_HELPER_EXTRA_FDS', None)
    started = time.perf_counter()
    with (folder/(program+'.log')).open('w') as stream:
        process = subprocess.run(command, stdout=stream, stderr=stream, env=env, cwd=folder,
                                 pass_fds=tuple(r._LEASES))
    with (folder/'commands.jsonl').open('a') as stream:
        stream.write(json.dumps({'program': program, 'returncode': process.returncode,
                                 'wall_seconds': time.perf_counter()-started})+'\n')
    return process.returncode


def edited_record_pair(gold_records, oracle_records, name):
    """Pair a unique target across original independent face orderings."""
    gold = [record for record in gold_records if any(face['name'] == name for face in record['faces'])]
    oracle = [record for record in oracle_records if any(face['name'] == name for face in record['cards'])]
    r.require(len(gold) == len(oracle) == 1, 'edited face must uniquely identify gold and Oracle records')
    gold_names = [face['name'] for face in gold[0]['faces']]
    oracle_names = [face['name'] for face in oracle[0]['cards']]
    r.require(gold_names.count(name) == oracle_names.count(name) == 1
              and sorted(gold_names) == sorted(oracle_names),
              'edited gold and Oracle records have ambiguous or different face identities')
    return copy.deepcopy(gold[0]), copy.deepcopy(oracle[0])


def edited_family_case(face, row, flags, *, allow_seeded):
    """Use the same contextual print and reparse contract in both lanes."""
    case = {'name': face['name'], 'text': row['text'], 'context': face['characteristics'],
            'expected': face['abilities'], 'readings': None, 'exact': bool(flags[1]), 'default': True}
    if allow_seeded:
        case['compositional'] = False
    return case


def edited_checks(run, directory, grammar, task, root, data):
    path = directory/'edits.json'
    if not path.is_file():
        path = root/task['edits']
    r.require(path.exists(), 'missing operand edits fixture')
    edits = r.read(path)
    r.require(2 <= len(edits) <= 8, 'provide two to eight operand edit tests')
    results, cases = [], []
    for number, edit in enumerate(edits):
        name = edit['face']
        r.require(name in task['target_faces'], 'operand edit is outside the assigned face population')
        r.require(isinstance(edit['text'], str) and len(edit['text']) <= 16384, 'invalid edited text')
        gold, oracle = edited_record_pair(data[1], data[2], name)
        names = [f['name'] for f in gold['faces']]
        face = next(f for f in gold['faces'] if f['name'] == name)
        r.require(1 <= len(edit['set']) <= 8, 'bound each edit to eight primitive operands')
        for change in edit['set']:
            edit_leaf(face['abilities'], change['path'], change['value'])
        next(f for f in oracle['cards'] if f['name'] == name)['text'] = edit['text']
        folder = directory/f'edited-{number}'
        folder.mkdir()
        r.jsonl(folder/'gold.jsonl', [gold])
        r.write(folder/'oracle.json', [oracle])
        r.write(folder/'slice.json', {'cards': names})
        r.execute(run/'inputs', folder, grammar, 'score',
                  [folder/'gold.jsonl', folder/'oracle.json', folder/'slice.json',
                   folder/'rows.jsonl', '--render', '--round-trip'], 2)
        rows = r.rows(folder/'rows.jsonl')
        row = next(row for row in rows if row['name'] == name)
        flags = r.flags(row)
        r.require(len(rows) == len(names) and row['status'] == 'equal'
                  and flags[0] and flags[2], 'edited operand lacks equal verified print')
        r.execute(run/'inputs', folder, grammar, 'validate-readings',
                  [folder/'gold.jsonl', folder/'rows.jsonl', folder/'native.json',
                   folder/'native-failures.jsonl'], 2)
        native = r.read(folder/'native.json')
        r.require(native['readings']['total'] == native['readings']['valid']
                  == native['readings']['canonical']
                  and native['source_gold']['valid'] == len(names),
                  'edited operands or synthesized readings failed native validation')
        results.append({'face': name, 'mutations': edit['set'], 'flags': flags})
        cases.append(edited_family_case(face, row, flags, allow_seeded=task.get('family_case_modes') is True))
    folder = directory/'edited-family'
    folder.mkdir()
    fixture = folder/'family.json'
    r.write(fixture, {'family': 'edited-operands', 'cases': cases})
    r.require(helper(run, folder, grammar, 'family-check', [fixture, '--compositional']) == 0,
              'edited operand family checks failed: '+str(folder/'family-check.log'))
    return results


def validate_author_family(family):
    """Keep every worker-authored family case compositional."""
    r.require(isinstance(family, dict) and isinstance(family.get('cases'), list),
              'family cases must be an array')
    cases = family['cases']
    r.require(4 <= len(cases) <= 64 and all(isinstance(case, dict) for case in cases)
              and any(case.get('reject') for case in cases)
              and any(case.get('expected') is not None for case in cases),
              'family needs positive, expected-IR and refusal controls')
    r.require(all(case.get('compositional') is None or case.get('compositional') is True
                  for case in cases), 'worker-authored family cases must be compositional')


def _preview_checks(run, directory, grammar, task):
    data = w.payload(run/'inputs')
    by_name = _cohort_index(data[0])
    targets = task.get('target_faces')
    primary = (_primary_cohort(by_name, data[1], task['cohort'], targets) if targets
               else cohort.close_cohort(by_name, data[1], {'names': task['cohort']}, already_closed=True))
    result = w.checks(run/'inputs', directory, grammar, primary['names'], data, 2)
    r.require(result['faces'] == primary['closure_occurrence_count'], 'preview omitted target occurrences')
    result['deferred_checks'] = ['full assigned cohort', 'edited operand checks']
    return result


def _check(run, task_id, coordinator=False, *, preview=False, prepared=None):
    r.require(not (coordinator and preview), 'coordinator diagnostics require the full assigned cohort')
    task, root, manifest = prepared if prepared is not None else verify(run, task_id)
    parent = (run/'evaluations'/task_id if coordinator else root/'.cache/author-checks')
    parent.mkdir(parents=True, exist_ok=True)
    lock = nullcontext(True) if prepared is not None else r.locked(root/'.cache/author-check.lock', blocking=False)
    with lock as acquired:
        r.require(acquired, 'a diagnostic helper is still running; retain and wait on its handle')
        attempts = list(parent.glob('attempt-*'))
        limit = task.get('max_diagnostic_attempts', 8)
        r.require(coordinator or len(attempts) < limit, 'diagnostic attempt budget exhausted; escalate')
        directory = parent/('attempt-'+uuid.uuid4().hex)
        directory.mkdir()
        started = time.perf_counter()
        report = {'task': task_id, 'source': manifest['commit'], 'passed': False,
                  'descriptor_sha256': r.digest(run/'tasks'/f'{task_id}.json'),
                  'support_sha256': r.digest(run/'support.json'),
                  'directory': str(directory), 'failures': []}
        if preview:
            report['scope'] = 'author-target-preview'
        before = r.tree(root/'grammar')
        shutil.copytree(root/'grammar', directory/'grammar')
        grammar = directory/'grammar'
        try:
            expected = seeded(run/'inputs/frozen')
            remaining = seeded(root)
            r.require(expected-remaining == set(task['rules']) and not remaining-expected,
                      'remove exactly assigned rules; never rename or adopt seeded rules')
            check_seeded_files(run/'inputs/frozen', root, task['rules'], directory)
            hooks_path = Path(task.get('hooks', '.cache/author-hooks.json'))
            r.require(not hooks_path.is_absolute() and hooks_path.parts and hooks_path.parts[0] == '.cache'
                      and '..' not in hooks_path.parts, 'hook requests belong in private .cache')
            hooks = r.read(root/hooks_path) if (root/hooks_path).exists() else []
            report['hooks'] = apply_hooks(grammar, hooks)
            r.write(directory/'hooks.json', hooks)
            fixture = root/task['fixture']
            validate_author_family(r.read(fixture))
            shutil.copy2(fixture, directory/'family.json')
            report['fixture_sha256'] = r.digest(fixture)
            edits = root/task['edits']
            if coordinator and edits.stat().st_size <= 1024*1024:
                archived_edits = directory/'edits.json'
                shutil.copyfile(edits, archived_edits)
                edits = archived_edits
            report['edits_sha256'] = r.digest(edits)
            report['family_returncode'] = helper(run, directory, grammar, 'family-check',
                                                  [directory/'family.json', '--compositional'])
            if report['family_returncode']:
                report['failures'].append('family regression tests failed')
                report['checks_skipped'] = {
                    'reason': 'authored compositional family regression failed',
                    'checks': ['cohort score', 'cohort native validation', 'edited operand checks'],
                }
            elif preview:
                report.update(_preview_checks(run, directory, grammar, task))
                report['passed'] = not report['failures']
            else:
                compact = screen_packet.load(run, task, r.digest(run/'inputs/manifest.json'),
                                             r.read(run/'support.json')) if 'screen_packet_sha256' in task else None
                data = None if compact is not None else w.payload(run/'inputs')
                screen = early_parse_screen(run, directory, grammar, task, data, compact=compact)
                if screen is not None:
                    report['early_parse_screen'] = screen
                if screen is not None and screen['status'] == 'failed':
                    report['failures'].extend(screen['failures'])
                    report['checks_skipped'] = {
                        'reason': 'primary parse-only screen detected a regression',
                        'checks': ['full cohort score', 'full cohort native validation',
                                   'full cohort print gates', 'edited operand checks'],
                    }
                else:
                    if data is None:
                        data = w.payload(run/'inputs')
                    # A bad edited operand can reject this trial before the full cohort score.
                    report['edited'] = edited_checks(run, directory, grammar, task, root, data)
                    report.update(w.checks(run/'inputs', directory, grammar, task['cohort'], data, 2))
                    report['passed'] = not report['failures']
        except Exception as error:
            report['failures'].append(str(error))
        verify(run, task_id)
        r.require(before == r.tree(root/'grammar'), 'worker changed grammar during diagnostics')
        report.update(grammar=r.tree(grammar), source_grammar=before,
                      wall_seconds=time.perf_counter()-started)
        r.write(directory/'report.json', report)
        print(json.dumps(report), flush=True)
        return report


def check(run, task_id, coordinator=False, *, preview=False):
    r.require(not (coordinator and preview), 'coordinator diagnostics require the full assigned cohort')
    prepared = verify(run, task_id)
    task, root, _ = prepared
    # A native child can retain the task lease after its model process exits.
    with r.locked(root/'.cache/author-check.lock', blocking=coordinator) as acquired:
        r.require(acquired, 'a diagnostic helper is still running; retain and wait on its handle')
        return _check_with_cpu(run, task_id, coordinator, preview, prepared)


def _check_with_cpu(run, task_id, coordinator, preview, prepared):
    task = prepared[0]
    if not task.get('cpu_pool'):
        return _check(run, task_id, coordinator, preview=preview, prepared=prepared)
    import retire_cpu
    with retire_cpu.reserve(Path(task['cpu_pool']), task.get('helper_threads', 2),
                            capacity=task.get('cpu_capacity', 32)) as fds:
        r._LEASES.extend(fds)
        r._CPU_LEASES = fds
        inherited = os.environ.get('RETIRE_JOB_FD')
        if inherited:
            r._LEASES.append(int(inherited))
        try:
            return _check(run, task_id, coordinator, preview=preview, prepared=prepared)
        finally:
            r._CPU_LEASES = ()
            if inherited:
                r._LEASES.remove(int(inherited))
            for fd in fds:
                r._LEASES.remove(fd)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    parser.add_argument('task')
    parser.add_argument('--coordinator', action='store_true')
    parser.add_argument('--preview', action='store_true')
    parser.add_argument('--remove-assigned', action='store_true')
    args = parser.parse_args()
    if args.preview and (args.coordinator or args.remove_assigned):
        parser.error('a target preview is an author diagnostic')
    if args.remove_assigned:
        task, root, _ = verify(args.run.resolve(), args.task)
        with r.locked(root/'.cache/author-check.lock', blocking=False) as acquired:
            r.require(acquired, 'a diagnostic helper is still running')
            result = regenerate_removals(args.run.resolve()/'inputs/frozen', root, task)
            verify(args.run.resolve(), args.task)
            print(json.dumps(result))
    else:
        check(args.run.resolve(), args.task, args.coordinator, preview=args.preview)


if __name__ == '__main__':
    main()
