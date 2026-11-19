#!/usr/bin/env python3
"""Reproduce an archived batch failure and prepare focused survivor evidence."""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import tempfile

import mined
import retire as r
import retire_author as author
import retire_batch as batch
import retire_cpu as cpu
import retire_merge as merge
import retire_native_outcome as native
import retire_parse_readmission as readmission
import retire_worker as worker
from retire_native_readmission import _allowed
from retire_queue import Queue, _gates, _proof_file


class FocusedTrialLimit(ValueError):
    def __init__(self, suspects):
        super().__init__('focused trial limit reached')
        self.suspects = tuple(suspects)


def _spec(path):
    return {'path': str(path), 'sha256': r.digest(path)}


def _archived(campaign, batch_id, *, full_check=False):
    r.require(re.fullmatch(r'[0-9a-f]{32}', batch_id), 'invalid batch ID')
    folder = campaign/'.cache/batches'/batch_id
    manifest = r.read(folder/'manifest.json')
    validation = r.read(folder/'validation.json')
    origin = manifest['base_commit']
    jobs = [row for row in manifest['jobs'] if row['status'] == 'included']
    reason = readmission.PRINT_ONLY_REJECTION if full_check else readmission.PARSE_SCREEN_REJECTION
    r.require(manifest['id'] == batch_id and 2 <= len(jobs) <= 96
              and len({row['job_id'] for row in jobs}) == len(jobs)
              and all(row['kind'] in ('author', 'mechanical') for row in jobs)
              and validation['status'] == 'failed' and validation['scope'] == 'combined-batch'
              and validation['reason'] == reason
              and validation['candidate'] == manifest['commit'] and validation['source_checkpoint'] == origin
              and validation['included_jobs'] == [row['job_id'] for row in jobs]
              and validation['repository_check_returncode'] == 0
              and all(x in ('passed', 'reused') for x in _gates((folder/'repository-check.log').read_bytes()).values()),
              'batch does not contain the selected complete rejected audit')
    reads = readmission.Reads(_proof_file)
    evidence = {}
    if full_check:
        parent = r.read(campaign/'checkpoints'/f'{origin}.json')
        evidence['archive_manifests'] = {'checkpoint_manifest_sha256': _spec(Path(parent['inputs'])/'manifest.json'),
                                         'validation_manifest_sha256': _spec(folder/'validation-inputs/manifest.json')}
        evidence['full_artifacts'] = {name: _spec(folder/'full-checks'/name) for name in readmission.ARTIFACTS}
    checkpoint, inputs, run, data, _ = readmission._frozen(
        campaign, origin, manifest, validation, reads, evidence.get('archive_manifests'))
    if full_check:
        _, _, names = readmission._original_full_checks(run, folder, validation, data, reads, evidence['full_artifacts'])
    else:
        additions, refusals, losses = readmission._original(run, folder, validation, data, reads)
        entries = list(data[0].values())
        names = ({row['name'] for row in additions}
                 | {entries[i]['name'] for i in refusals} | {entries[i]['name'] for i, _ in losses})
    r.require(names and len(names) <= 500, 'unsupported isolation population')
    reads.recheck()
    return folder, manifest, jobs, checkpoint, inputs, run, sorted(names), evidence


def _proposals(campaign, jobs, scratch, cached):
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    proposals = []
    with closing(sqlite3.connect(uri, uri=True)) as db:
        db.execute('BEGIN')
        for job in jobs:
            row = db.execute('SELECT job_id, result FROM attempts WHERE id=?', (job['attempt_id'],)).fetchone()
            r.require(row and row[0] == job['job_id'] and row[1], 'missing archived proposal')
            result = json.loads(row[1])
            original = result['evidence']
            report_path = Path(original['report'])
            report = r.read(report_path)
            r.require(result['status'] == 'provisional-pass' and original['kind'] == job['kind']
                      and report['descriptor_sha256'] == job['descriptor_sha256'], 'archived proposal changed')
            run, task, directory, hooks = batch._proposal(
                {**original, 'report': report}, input_cache=cached)
            rules = task['rules']
            r.require(task['id'] == job['job_id'] and rules == job['rules'], 'archived rule ownership changed')
            if job['kind'] == 'mechanical':
                r.require(not hooks, 'mechanical proposal contains hooks')
                batch._mechanical_removal(run, directory, rules, scratch)
            proposals.append((run, directory, rules, hooks, _spec(report_path)))
    r.require(cached.unchanged(), 'archived proposal input changed')
    return proposals


def _apply(root, job, proposal, cached):
    run, directory, rules, hooks, _ = proposal
    if job['kind'] == 'author':
        files = merge.merge_grammar(run/'inputs/frozen', root, directory, rules, hooks,
                                    canonical_removal=cached.canonical_seeded_removal)
        for name, text in files.items():
            path = root/name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    removed, retained = mined.remove(root, rules)
    r.require(set(removed) == set(rules) and not retained, 'proposal has retained rule users')
    author.apply_hooks(root/'grammar', hooks)


def _materialize(root, grammar, selection, applied, proposals, cached):
    shutil.copytree(grammar, root/'grammar')
    for index in applied:
        job, proposal = proposals[index]
        _apply(root, job, proposal, cached)
    r.require(cached.unchanged(), 'archived proposal inputs changed during replay')
    tree = r.tree(root/'grammar')
    assigned = [{k: proposals[i][0][k] for k in ('job_id', 'attempt_id')} for i in selection]
    r.write(root/'identity.json', {'method': 'ordered-proposal-replay-v1', 'indices': selection,
                                  'tree': tree, 'proposals': assigned})
    return tree


def _check(checkpoint, control, root):
    index, gold, oracle = control['data']
    selected = set(control['names'])
    inputs = control['run']
    subset = ({k: v for k,v in index.items() if v['name'] in selected}, gold, oracle)
    checks = root/'checks'
    checks.mkdir()
    with cpu.reserve(Path(checkpoint['cpu_pool']), 2, checkpoint['cpu_capacity'], background=True) as fds:
        r._LEASES.extend(fds)
        try:
            try:
                result = worker.checks(inputs, checks, root/'grammar', control['names'], subset, 2)
            except subprocess.CalledProcessError as error:
                result = native.preservation_failure(error, inputs, checks, subset)
        finally:
            for fd in fds:
                r._LEASES.remove(fd)
    r.require(result['faces'] == control['occurrences'], 'focused result does not cover its complete closure')
    r.write(root/'result.json', result)
    return result


def _isolate(scope, trial, prefix):
    if not scope or not trial(sorted(prefix+scope))['failures']:
        return []
    if len(scope) == 1:
        return scope
    middle = len(scope)//2
    left, right = scope[:middle], scope[middle:]
    separate = _isolate(left, trial, prefix) + _isolate(right, trial, prefix)
    # Both halves can pass separately. Keep the larger half and verify the recombined survivors.
    return separate or (left if len(left) < len(right) else right)


def _survivors(indices, trial, pinned, suspect=None, *, suspects=()):
    """Keep excluding witnessed failures until the remaining union passes."""
    remaining = list(indices)
    if suspect is not None and suspect not in pinned:
        without = [index for index in remaining if index != suspect]
        if without and not trial(without)['failures']:
            return without
    witnessed = {index for index in suspects if index not in pinned and trial(sorted([*pinned, index]))['failures']}
    remaining = [index for index in remaining if index not in witnessed]
    while remaining and trial(remaining)['failures']:
        excluded = _isolate([i for i in remaining if i not in pinned], trial, pinned)
        r.require(excluded, 'focused failure could not be separated from the protected prefix')
        remaining = [i for i in remaining if i not in excluded]
    return remaining


def _witnessed_suspects(seen, pinned, jobs):
    pinned = set(pinned)
    suspects = set()
    for selection, item in seen.items():
        selected = set(selection)
        candidates = selected - pinned
        if len(candidates) == 1 and pinned <= selected and item['result']['failures']:
            suspects.update(candidates)
    return [jobs[i]['job_id'] for i in sorted(suspects)]


def _controls(roots):
    return {label: {'identity': _spec(root/'identity.json'), 'result': _spec(root/'result.json'),
                    'population': _spec(root/'checks/slice.json'),
                    'artifacts': {name: _spec(root/'checks'/name) for name in readmission.ARTIFACTS}}
            for label, root in roots.items()}


def _probe_lines(binary, grammar, name, text, fds):
    env = {k:v for k,v in os.environ.items() if not k.startswith(('ORACLE_GRAMMAR_', 'PROBE_'))}
    env['ORACLE_GRAMMAR_DIR'] = str(grammar)
    with tempfile.TemporaryFile() as output:
        subprocess.run([str(binary), name, text], env=env, stdout=output, stderr=subprocess.DEVNULL,
                       check=True, timeout=12, pass_fds=fds)
        output.seek(0)
        result = output.read(1024*1024+1)
    r.require(len(result) <= 1024*1024, 'parse trace exceeds the hint budget')
    return result.splitlines()


def _route_names(line):
    return {key.split('.',1)[0].split('#',1)[0]
            for key in json.loads(line.removeprefix(b'  rules '))}


def _probe_rules(binary, grammar, name, text, fds):
    traces = [_route_names(line) for line in _probe_lines(binary,grammar,name,text,fds)
              if line.startswith(b'  rules ')]
    return set().union(*traces)


def _new_productions(run, directory, wanted):
    found = set()
    for path in (directory/'grammar/syntax').glob('*.ebnf'):
        if path.name == 'mined.ebnf':
            continue
        source = path.read_text()
        if not any(name+' ::=' in source for name in wanted):
            continue
        base = run/'inputs/frozen/grammar/syntax'/path.name
        previous = mined.definitions(base.read_text().splitlines()) if base.exists() else {}
        found.update((wanted & mined.definitions(source.splitlines()).keys()) - previous.keys())
    return found


def _trace_binary(checkpoint, run):
    binary = Path(checkpoint['helpers'])/'probe'
    expected = r.read(run/'frozen/build.json')['binaries']['probe']
    r.require(binary.is_absolute() and binary.is_file() and not binary.is_symlink()
              and binary.stat().st_size <= 1024*1024*1024 and r.digest(binary) == expected,
              'probe differs from the frozen helper')
    return binary, expected


def _print_face_hints(checkpoint, binary, grammar, row, proposals, eligible, fds):
    original, printed = row.get('text'), row.get('first')
    if (row.get('readings') != 1 or 'matching_readings' in row
            or not all(isinstance(text, str) and 0 < len(text) <= 4096 for text in (original, printed))
            or original == printed):
        return []
    name = row['name']
    spelled = _probe_rules(binary, grammar, name, printed, fds)
    retained = _probe_rules(binary, grammar, name, original, fds)
    wanted = spelled - retained
    matches = [i for i in eligible if wanted and proposals[i][0]['kind'] == 'author'
               and _new_productions(*proposals[i][1][:2], wanted)]
    if len(matches) == 1 or not checkpoint.get('inputs'):
        return matches if len(matches) == 1 else []
    accepted = Path(checkpoint['inputs'])/'frozen/grammar'
    before = _probe_rules(binary, accepted, name, original, fds)
    removed = {route for route in before-retained if mined.SEEDED.fullmatch(route)}
    return [i for i in eligible if removed.intersection(proposals[i][0]['rules'])]


def _print_hints(checkpoint, run, full, proposals, eligible, failures):
    """Use changed print traces to prioritize audited focused trials."""
    if not 1 <= len(failures) <= 8 or any(not isinstance(item, dict)
            or item.get('lost') != ['exact_default'] or not isinstance(item.get('face'), str) for item in failures):
        return []
    names = {item['face'] for item in failures}
    rows = [row for row in r.rows(full/'checks/rows.jsonl') if row['name'] in names]
    if len(names) != len(failures) or len(rows) != len(names) or {row['name'] for row in rows} != names:
        return []
    try:
        binary, expected = _trace_binary(checkpoint, run)
        matches = set()
        with cpu.reserve(Path(checkpoint['cpu_pool']), 1, checkpoint['cpu_capacity'], background=True) as fds:
            for row in rows:
                try:
                    matches.update(_print_face_hints(checkpoint, binary, full/'grammar',
                                                     row, proposals, eligible, fds))
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                    continue
        return sorted(matches) if 0 < len(matches) <= 8 and r.digest(binary) == expected else []
    except (OSError, ValueError, KeyError, TypeError):
        return []


def _added_trace_rules(lines, row, delta):
    meanings = []
    for line in lines:
        if line.startswith(b'[{'):
            meanings.append((json.loads(line),set()))
        elif line.startswith(b'  rules ') and meanings:
            meanings[-1][1].update(_route_names(line))

    def rules(indices, *, require_route):
        readings = row['all']
        wanted = set()
        for index in indices:
            r.require(type(index) is int and 0 <= index < len(readings), 'invalid trace reading index')
            trace, = [route for meaning,route in meanings if meaning == readings[index]]
            r.require(trace or not require_route, 'empty trace for added native reading')
            wanted.update(trace)
        return wanted

    added = rules(delta['added_current_indices'],require_route=True)
    retained = rules(delta['retained_current_indices'],require_route=False)
    return added-retained


def _parse_hints(checkpoint, run, full, proposals, eligible):
    """Prioritize proposals named by added native routes; focused trials still decide exclusions."""
    try:
        native = r.read(full/'checks/native.json')['reading_preservation']
        deltas = native['additions']
        if not 1 <= len(deltas) <= 8 or native.get('missing') or native.get('merged_faces'):
            return []
        names = {delta['name'] for delta in deltas}
        rows = [row for row in r.rows(full/'checks/rows.jsonl') if row['name'] in names]
        if len(names) != len(deltas) or len(rows) != len(names) or {row['name'] for row in rows} != names:
            return []
        binary, expected = _trace_binary(checkpoint,run)
        evidence = {row['name']:row for row in rows}
        wanted = set()
        with cpu.reserve(Path(checkpoint['cpu_pool']),1,checkpoint['cpu_capacity'],background=True) as fds:
            for delta in deltas:
                row = evidence[delta['name']]
                if not 0 < len(row['text']) <= 4096:
                    continue
                try:
                    lines = _probe_lines(binary,full/'grammar',row['name'],row['text'],fds)
                    wanted.update(_added_trace_rules(lines,row,delta))
                except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
                    continue
        if not wanted or r.digest(binary) != expected:
            return []
        matches = [index for index in eligible if proposals[index][0]['kind'] == 'author'
                   and _new_productions(*proposals[index][1][:2],wanted)]
        return matches if len(matches) <= 8 else []
    except (OSError, ValueError, KeyError, TypeError):
        return []


def _completed_prefix(selection, seen):
    for count in range(len(selection)-1, 0, -1):
        cached = seen.get(tuple(selection[:count]))
        if cached is not None:
            grammar = cached['root']/'grammar'
            r.require(r.tree(grammar) == cached['tree'], 'cached grammar prefix changed')
            return grammar, selection[count:]
    return None


def isolate(campaign, batch_id, scratch, max_trials, *, suspect=None, suspects=(), full_check=False,
            witness_retry=0, unavailable=()):
    r.require(3 <= max_trials <= 96 and 0 <= witness_retry <= 96-max_trials,
              'invalid focused trial budget')
    folder, manifest, jobs, checkpoint, inputs, run, names, archived = _archived(campaign, batch_id, full_check=full_check)
    by_job = {job['job_id']: i for i, job in enumerate(jobs)}
    hints = [suspect] if suspect is not None else list(suspects)
    r.require((suspect is None or not suspects) and len(hints) == len(set(hints)) and set(hints) <= by_job.keys(),
              'suspects must be distinct included proposals')
    unavailable = set(unavailable)
    r.require(unavailable < by_job.keys() and not unavailable.intersection(hints),
              'unavailable proposals must be an included subset disjoint from suspects')
    r.require(_allowed(campaign, scratch) and not scratch.exists(), 'private isolation directory already exists or is outside scratch')
    scratch.mkdir(parents=True)
    profile = scratch/'profile.json'
    r.write(profile, {'cards': names})
    cp = dict(checkpoint, native_regression_controls=_spec(profile), native_control_threads=2)
    # Use the accepted audit rows frozen for the rejected candidate, including
    # any print flags improved after the checkpoint inputs were captured.
    cp.update(commit=manifest['commit'], inputs=str(run))
    control = batch._control_setup(cp)
    cached = batch._ProposalInputCache()
    with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
        proposals = list(zip(jobs, _proposals(campaign, jobs, scratch, cached), strict=True))
    indices = list(range(len(jobs)))
    eligible = [i for i in indices if jobs[i]['job_id'] not in unavailable]
    mechanics = [i for i in indices if jobs[i]['kind'] == 'mechanical']
    prefix = mechanics if mechanics == indices[:len(mechanics)] else []
    seen, roots = {}, {}
    pinned = []
    limit = max_trials

    def trial(selection, label=None):
        key = tuple(selection)
        if key in seen:
            saved = seen[key]['root']
            if label == 'full':
                r.require(r.tree(saved/'grammar') == manifest['grammar'],
                          'complete ordered replay differs from rejected batch')
            if label:
                roots[label] = saved
            return seen[key]['result']
        if len(seen) >= limit:
            raise FocusedTrialLimit(_witnessed_suspects(seen, pinned, jobs))
        root = scratch/(label or f'trial-{len(seen):03d}')
        frozen = inputs/'frozen/grammar'
        remainder = selection
        with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
            if full_check and label == 'full':
                frozen, remainder = run/'frozen/grammar', []
            elif (ready := _completed_prefix(selection, seen)) is not None:
                frozen, remainder = ready
            tree = _materialize(root, frozen, selection, remainder, proposals, cached)
        if label == 'full':
            r.require(tree == manifest['grammar'], 'complete ordered replay differs from rejected batch')
        result = _check(cp, control, root)
        r.require(r.tree(root/'grammar') == tree, 'focused grammar changed during validation')
        if label:
            roots[label] = root
        seen[key] = {'root': root, 'tree': tree, 'result': result}
        r.write(scratch/'progress.json', [{'indices': list(key), 'folder': str(item['root']), **item['result']}
                                        for key, item in seen.items()])
        return result

    r.require(not trial([], 'base')['failures'], 'accepted base does not pass focused checks')
    pinned = prefix if prefix and prefix == eligible[:len(prefix)] and not trial(prefix, 'mechanical')['failures'] else []
    failed = trial(indices, 'full')['failures']
    r.require(failed, 'combined audit failure did not reproduce')
    if full_check:
        r.require(failed == r.read(folder/'validation.json')['full_checks']['failures'],
                  'focused full audit did not reproduce the exact failures')
    routed = [by_job[job] for job in hints]
    automatic = []
    if not routed:
        if full_check:
            automatic = _print_hints(checkpoint, run, roots['full'], proposals, eligible, failed)
        else:
            automatic = _parse_hints(checkpoint,run,roots['full'],proposals,eligible)
    try:
        remaining = (_survivors(eligible, trial, pinned, routed[0]) if len(routed) == 1
                     else _survivors(eligible, trial, pinned, suspects=routed or automatic))
    except FocusedTrialLimit as exhausted:
        if not exhausted.suspects or not witness_retry:
            raise
        # The witnesses and both baseline trials already ran against these
        # immutable inputs. Keep their exact results and prove the survivor union.
        limit += witness_retry
        remaining = _survivors(eligible, trial, pinned, suspects=[by_job[job] for job in exhausted.suspects
                                                                 if job not in unavailable])
    excluded = [i for i in indices if i not in remaining]
    r.require(excluded and remaining and not trial(remaining, 'remaining')['failures'],
              'there is no nonempty survivor union with passing focused checks')
    original_reports = scratch/'original-reports.json'
    reports = {job['job_id']: proposal[-1] for job, proposal in proposals}
    r.write(original_reports, reports)
    isolation = scratch/'isolation.json'
    mode = 'full-check' if full_check else 'parse-screen'
    control_baseline = {} if full_check else {'control_baseline': readmission.VALIDATION_INPUTS}
    r.write(isolation, {**control_baseline, 'kind': f'isolated-{mode}-regressions', 'manifest': batch_id,
                        'candidate': manifest['commit'], 'parent': checkpoint['commit'],
                        'excluded_jobs': [jobs[i]['job_id'] for i in excluded],
                        'reason': 'The retained union preserved every reading and print gate on the failed face closure.',
                        'profile': _spec(profile), 'population_count': control['occurrences'],
                        'controls': _controls({k: roots[k] for k in ('base','full','remaining')}),
                        'original_reports': _spec(original_reports)})
    selected = [jobs[i]['job_id'] for i in remaining]
    evidence = {**archived, 'kind': f'isolated-{mode}-revalidation', 'source_checkpoint': checkpoint['commit'],
                'manifest': _spec(folder/'manifest.json'), 'validation': _spec(folder/'validation.json'),
                'repository_log': _spec(folder/'repository-check.log'), 'isolation': _spec(isolation),
                'reports': {job: reports[job] for job in selected}}
    result = {'considered': len(jobs), 'jobs': selected, 'evidence': evidence,
              'rules': sum(len(jobs[i]['rules']) for i in remaining)}
    r.write(scratch/'result.json', result)
    return result


def readmit_result(campaign, scratch, result, *, full_check=False):
    queue = Queue(campaign/'queue.sqlite')
    readmit = queue.readmit_full_check_batch_proposals if full_check else queue.readmit_parse_screen_batch_proposals
    return _readmit(queue, scratch, result, readmit)


def _readmit(queue, scratch, result, readmit):
    for retry in range(4):
        head = queue.progress()['checkpoint']
        try:
            receipt = readmit(result['jobs'], head, result['evidence'])
            break
        except ValueError:
            if queue.progress()['checkpoint'] == head or retry == 3:
                raise
    r.write(scratch/'readmitted.json', receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('batch')
    parser.add_argument('scratch', type=Path)
    parser.add_argument('--max-trials', type=int, default=24)
    parser.add_argument('--suspect', action='append', default=[], help='prioritize an included job in isolation; repeatable')
    parser.add_argument('--full-check', action='store_true', help='isolate a rejected full native and print audit')
    parser.add_argument('--apply', action='store_true', help='verify and return survivors to fresh validation')
    args = parser.parse_args()
    if not 3 <= args.max_trials <= 96:
        parser.error('--max-trials must be between 3 and 96')
    campaign, scratch = args.campaign.resolve(), args.scratch.resolve()
    result = isolate(campaign, args.batch, scratch, args.max_trials, suspects=args.suspect, full_check=args.full_check)
    if args.apply:
        readmit_result(campaign, scratch, result, full_check=args.full_check)
    print(json.dumps({'rules': result['rules'], 'jobs': len(result['jobs']), 'proof': str(scratch/'result.json')}))


if __name__ == '__main__':
    main()
