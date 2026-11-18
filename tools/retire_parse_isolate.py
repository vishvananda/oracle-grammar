#!/usr/bin/env python3
"""Reproduce an archived batch failure and prepare focused survivor evidence."""
import argparse
from contextlib import closing
import json
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess

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


def _controls(roots):
    return {label: {'identity': _spec(root/'identity.json'), 'result': _spec(root/'result.json'),
                    'population': _spec(root/'checks/slice.json'),
                    'artifacts': {name: _spec(root/'checks'/name) for name in readmission.ARTIFACTS}}
            for label, root in roots.items()}


def isolate(campaign, batch_id, scratch, max_trials, *, suspect=None, suspects=(), full_check=False):
    folder, manifest, jobs, checkpoint, inputs, run, names, archived = _archived(campaign, batch_id, full_check=full_check)
    by_job = {job['job_id']: i for i, job in enumerate(jobs)}
    hints = [suspect] if suspect is not None else list(suspects)
    r.require((suspect is None or not suspects) and len(hints) == len(set(hints)) and set(hints) <= by_job.keys(),
              'suspects must be distinct included proposals')
    r.require(_allowed(campaign, scratch) and not scratch.exists(), 'private isolation directory already exists or is outside scratch')
    scratch.mkdir(parents=True)
    profile = scratch/'profile.json'
    r.write(profile, {'cards': names})
    cp = dict(checkpoint, native_regression_controls=_spec(profile), native_control_threads=2)
    if full_check:
        # The rejected candidate froze the accepted audit's print flags. Older
        # checkpoint input indices can have the same readings with older flags.
        cp.update(commit=manifest['commit'], inputs=str(run))
    control = batch._control_setup(cp)
    cached = batch._ProposalInputCache()
    with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
        proposals = list(zip(jobs, _proposals(campaign, jobs, scratch, cached), strict=True))
    indices = list(range(len(jobs)))
    mechanics = [i for i in indices if jobs[i]['kind'] == 'mechanical']
    prefix = mechanics if mechanics == indices[:len(mechanics)] else []
    seen, roots = {}, {}

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
        r.require(len(seen) < max_trials, 'focused trial limit reached')
        root = scratch/(label or f'trial-{len(seen):03d}')
        frozen = inputs/'frozen/grammar'
        remainder = selection
        with cpu.reserve(Path(cp['cpu_pool']), 1, cp['cpu_capacity'], background=True):
            ready = seen.get(tuple(prefix))
            if full_check and label == 'full':
                frozen, remainder = run/'frozen/grammar', []
            elif prefix and ready and selection[:len(prefix)] == prefix:
                saved = ready['root']
                frozen = saved/'grammar'
                r.require(r.tree(frozen) == r.read(saved/'identity.json')['tree'], 'cached grammar prefix changed')
                remainder = selection[len(prefix):]
            tree = _materialize(root, frozen, selection, remainder, proposals, cached)
        if label == 'full':
            r.require(tree == manifest['grammar'], 'complete ordered replay differs from rejected batch')
        result = _check(cp, control, root)
        r.require(r.tree(root/'grammar') == tree, 'focused grammar changed during validation')
        if label:
            roots[label] = root
        seen[key] = {'root': root, 'result': result}
        r.write(scratch/'progress.json', [{'indices': list(key), 'folder': str(item['root']), **item['result']}
                                        for key, item in seen.items()])
        return result

    r.require(not trial([], 'base')['failures'], 'accepted base does not pass focused checks')
    pinned = prefix if prefix and not trial(prefix, 'mechanical')['failures'] else []
    failed = trial(indices, 'full')['failures']
    r.require(failed, 'combined audit failure did not reproduce')
    if full_check:
        r.require(failed == r.read(folder/'validation.json')['full_checks']['failures'],
                  'focused full audit did not reproduce the exact failures')
    routed = [by_job[job] for job in hints]
    remaining = (_survivors(indices, trial, pinned, routed[0]) if len(routed) == 1
                 else _survivors(indices, trial, pinned, suspects=routed))
    excluded = [i for i in indices if i not in remaining]
    r.require(excluded and remaining and not trial(remaining, 'remaining')['failures'],
              'there is no nonempty survivor union with passing focused checks')
    original_reports = scratch/'original-reports.json'
    reports = {job['job_id']: proposal[-1] for job, proposal in proposals}
    r.write(original_reports, reports)
    isolation = scratch/'isolation.json'
    mode = 'full-check' if full_check else 'parse-screen'
    r.write(isolation, {'kind': f'isolated-{mode}-regressions', 'manifest': batch_id,
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
