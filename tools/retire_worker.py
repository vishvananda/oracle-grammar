"""Execution side of retire.py; no builds, source edits, commits or publication."""
from contextlib import ExitStack
import json
from pathlib import Path
import re
import shutil
import uuid

import mined
import retire as r


def cohort(run, rule, index):
    """A diagnostic cohort, explicitly not a proof of print independence."""
    entries = []
    for path in mined.grammar_files(run/'frozen'):
        found = mined.definitions(path.read_text().splitlines())
        if rule in found:
            entries.append(found[rule])
    r.require(len(entries) == 1, 'candidate must have exactly one declaration')
    body = '\n'.join(line for _, line in entries[0]).split('::=', 1)[1]
    # Branching surfaces need a grammar-aware dependency index; do not guess
    # that all alternative words must occur in the same input.
    unquoted = mined.QUOTED.sub('', body)
    if any(mark in unquoted for mark in '|?*+()'):
        return []
    words = re.findall(r'\w+', ' '.join(json.loads(q) for q in mined.QUOTED.findall(body)).lower())
    if not words:
        return []
    def matches(text):
        tokens = iter(re.findall(r'\w+', text.lower()))
        return all(any(token == word for token in tokens) for word in words)
    return sorted(name for name, entry in index.items() if matches(entry['text']))


def payload(run):
    return (r.read(run/'frozen/index.json'), r.rows(run/'frozen/gold.jsonl'),
            r.read(run/'frozen/oracle.json'))


def subset(directory, names, data):
    index, gold, oracle = data
    selected = set(names)
    gold = [record for record in gold if any(f['name'] in selected for f in record['faces'])]
    # Keep a card's companion faces so original-source metadata joins by the
    # same complete face-name tuple. All companions were explicitly allowed.
    selected = {f['name'] for record in gold for f in record['faces']}
    oracle = [record for record in oracle if any(f['name'] in selected for f in record['cards'])]
    r.require(selected and selected <= index.keys(), 'cohort escaped the allowed population')
    r.jsonl(directory/'gold.jsonl', gold)
    r.write(directory/'oracle.json', oracle)
    r.write(directory/'slice.json', {'cards': sorted(selected)})
    r.jsonl(directory/'before.jsonl', [index[name]['parse'] for name in sorted(selected)])
    return selected


def native_failures(report, count):
    preservation = report['reading_preservation']
    readings = report['readings']
    failures = []
    if not (preservation['passed'] and not preservation['missing']
            and not preservation['additions'] and preservation['merged_faces'] == 0
            and preservation['old_readings'] == readings['total']
            == readings['valid'] == readings['canonical']):
        failures.append('native readings changed or failed validation')
    if not (report['source_gold']['valid'] == report['canonical_gold']['valid']
            == report['canonical_gold']['idempotent'] == count):
        failures.append('gold validation is incomplete')
    return failures


def compare(current, index, selected):
    r.require(len(current) == len(selected) and {row['name'] for row in current} == selected,
              'score did not cover the exact cohort')
    failures = []
    for row in current:
        old = index[row['name']]
        if (row['status'] != 'equal' or row['text'] != old['text']
                or row['readings'] != old['parse']['readings']):
            failures.append({'face': row['name'], 'gate': 'parse/reading-count/text'})
        now = r.flags(row)
        lost = [gate for gate, before, after in zip(r.PRINT_GATES,
                                                   old['flags'], now) if before and not after]
        if lost:
            failures.append({'face': row['name'], 'lost': lost})
    return failures


def checks(run, directory, grammar, names, data, threads):
    selected = subset(directory, names, data)
    r.execute(run, directory, grammar, 'score',
              [directory/'gold.jsonl', directory/'oracle.json', directory/'slice.json',
               directory/'rows.jsonl', '--render', '--round-trip'], threads)
    r.execute(run, directory, grammar, 'validate-readings',
              [directory/'gold.jsonl', directory/'rows.jsonl', directory/'native.json',
               directory/'native-failures.jsonl', '--preserve-from', directory/'before.jsonl'],
              threads)
    failures = native_failures(r.read(directory/'native.json'), len(selected))
    failures.extend(compare(r.rows(directory/'rows.jsonl'), data[0], selected))
    return {'faces': len(selected), 'failures': failures}


def job_inputs(run, job):
    description = r.read(job/'job.json')
    scan = r.read(run/'scan-complete.json')['report']
    r.require(description['snapshot'] == r.digest(run/'manifest.json')
              and description['scan'] == scan == r.digest(run/'scan/rules.json')
              and description['rule'] in r.read(run/'scan/rules.json')['unused'],
              'job is stale or is not an audited candidate')
    return description


def attempt_path(job, result):
    identity = result['attempt']
    r.require(re.fullmatch('[0-9a-f]{32}', identity), 'invalid attempt identity')
    return job/'attempts'/identity


def perform(run, job, name, data, manifest):
    description = job_inputs(run, job)
    result = {'worker': name, 'rule': description['rule'], 'snapshot': description['snapshot'],
              'status': 'running', 'attempt': uuid.uuid4().hex}
    r.write(job/'result.json', result)
    candidate = attempt_path(job, result)
    candidate.mkdir(parents=True)
    try:
        shutil.copytree(run/'frozen/grammar', candidate/'grammar')
        removed, kept = mined.remove(candidate, [description['rule']])
        grammar = r.tree(candidate/'grammar')
        names = cohort(run, description['rule'], data[0])
        if kept or not removed or not names or len(names) > 256:
            result.update(status='needs-review', reason='dependency or uncertain/large cohort',
                          kept=kept, cohort=len(names))
        else:
            report = checks(run, candidate, candidate/'grammar', names, data, manifest['threads'])
            result.update(report, status='needs-review' if report['failures'] else 'provisional-pass')
        r.verify(run)
        r.require(grammar == r.tree(candidate/'grammar'), 'candidate changed during validation')
        result['grammar'] = grammar
    except Exception as error:
        result.update(status='failed', reason=str(error))
    r.write(job/'result.json', result)
    return result


def worker(args):
    run = args.run.resolve()
    manifest = r.verify(run)
    r.require(1 <= args.limit <= 1000, 'worker limit must be between 1 and 1000')
    r.require((run/'scan-complete.json').is_file(), 'coordinator scan has not completed')
    with ExitStack() as leases:
        slot = next((n for n in range(manifest['slots']) if leases.enter_context(
            r.locked(run/'slots'/str(n), blocking=False))), None)
        if slot is None:
            print(json.dumps({'status': 'capacity-full'}))
            return
        data = payload(run)
        completed = 0
        for job in sorted((run/'jobs').iterdir()):
            if completed >= args.limit:
                break
            with ExitStack() as claim:
                # Assembly holds this barrier while checking every job lease.
                # No new claim can race the closing of the worker phase.
                with r.locked(run/'coordinator.lock', blocking=False) as coordinator:
                    if not coordinator or (run/'combined').exists():
                        print(json.dumps({'status': 'coordinator-busy-or-batch-closed'}))
                        return
                    acquired = claim.enter_context(r.locked(job/'lease', blocking=False))
                    if not acquired:
                        continue
                    if ((job/'result.json').exists()
                            and r.read(job/'result.json')['status'] != 'running'):
                        continue
                result = perform(run, job, args.name, data, manifest)
                print(json.dumps({key: result[key] for key in ('rule', 'status', 'faces', 'reason')
                                  if key in result} | {
                    'failures': len(result.get('failures', [])),
                    'report': str(job/'result.json')}), flush=True)
                completed += 1


def validate(args):
    run = args.run.resolve()
    manifest = r.verify(run)
    with r.locked(run/'coordinator.lock'):
        combined = run/'combined'
        selection = r.read(combined/'selection.json')
        r.require(selection['snapshot'] == r.digest(run/'manifest.json')
                  and selection['grammar'] == r.tree(combined/'grammar'), 'combined input changed')
        directory = combined/'validation'
        directory.mkdir()  # Existing or running validations are never restarted implicitly.
        report = checks(run, directory, combined/'grammar', r.read(run/'frozen/allowed.json')['cards'],
                        payload(run), manifest['slots'] * manifest['threads'])
        r.verify(run)
        r.require(selection['grammar'] == r.tree(combined/'grammar'), 'grammar changed during audit')
        report.update(status='validated' if not report['failures'] else 'needs-review',
                      snapshot=selection['snapshot'], grammar=selection['grammar'],
                      patch=r.digest(combined/'retirement.patch'))
        r.write(combined/'validation.json', report)
        print(json.dumps({'status': report['status'], 'faces': report['faces'],
                          'failures': len(report['failures']),
                          'report': str(combined/'validation.json')}))
        r.require(not report['failures'], 'combined validation failed; do not integrate')


def status(args):
    run = args.run.resolve()
    r.verify(run)
    counts = {}
    for job in (run/'jobs').iterdir():
        state = r.read(job/'result.json')['status'] if (job/'result.json').exists() else 'pending'
        with r.locked(job/'lease', blocking=False) as acquired:
            if state == 'running' and acquired:
                state = 'interrupted'
        counts[state] = counts.get(state, 0) + 1
    print(json.dumps({'jobs': counts, 'integration': 'never automatic'}))
