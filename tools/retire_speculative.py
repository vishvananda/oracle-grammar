#!/usr/bin/env python3
"""Stage the next batch on an in-flight candidate so it is ready when that candidate is accepted."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True

import retire as r
import retire_batch
import retire_batch_driver as driver
import retire_dispatch as dispatch

RECEIPT = 'batch-speculative-ready.json'


def _alive(pid):
    try:
        os.kill(pid, 0)
    except (ProcessLookupError, PermissionError, TypeError, OverflowError):
        return False
    return True


def in_flight(run):
    """Return the candidate whose parse screen passed while its full print still runs."""
    state = r.read(run/'batch-driver.json') if (run/'batch-driver.json').is_file() else {}
    head = r.read(run/'latest-checkpoint.json')['commit']
    if state.get('phase') != 'running' or state.get('checkpoint') != head or not _alive(state.get('pid')):
        return None
    batches = run/'.cache/batches'
    recent = sorted((p for p in batches.iterdir() if p.is_dir()), key=lambda p: p.stat().st_mtime, reverse=True)[:16]
    for folder in recent:
        manifest_path, screen = folder/'manifest.json', folder/'parse-screen/result.json'
        if not manifest_path.is_file() or not screen.is_file() or (folder/'validation.json').is_file():
            continue
        manifest = r.read(manifest_path)
        inputs = folder/'validation-inputs'
        if (manifest.get('status') == 'provisional' and manifest.get('base_commit') == head
                and manifest.get('id') == folder.name and not manifest.get('speculative')
                and r.read(screen).get('status') == 'passed' and (inputs/'manifest.json').is_file()
                and r.read(inputs/'manifest.json').get('commit') == manifest['commit']):
            return state, manifest, inputs
    return None


def next_selection(run, config, state, candidate):
    """Predict the lane the driver takes after this candidate: only an exact preflight plan qualifies."""
    every = config.get('stale_provisional_every', 4)
    since = 0 if state.get('lane') == 'stale-provisional' else state.get('batches_since_stale', 0)+1
    if config.get('stale_provisional_hours') is not None and since+1 >= every:
        return None
    spec = driver._preflight_receipt(run, config)
    if not isinstance(spec, dict) or set(spec) != {'path', 'sha256'}:
        return None
    plan = driver._preflight_document(Path(spec['path']), 128*1024, sha256=spec['sha256'])
    limit = config.get('verified_ready_batch_size', config['batch_size'])
    if (not isinstance(plan, dict) or plan.get('kind') != 'prospective-retirement-batch'
            or plan.get('checkpoint') != candidate['commit'] or not 1 <= len(plan.get('jobs', ())) <= limit):
        return None
    return spec, [(entry['job'], entry['attempt']) for entry in plan['jobs']]


def checkpoint_for(run, config, candidate, inputs):
    """Describe the candidate exactly as the driver will record it once accepted."""
    current = r.read(run/'latest-checkpoint.json')
    cp = dict(current, commit=candidate['commit'], source=candidate['source'], inputs=str(inputs),
              background_cpu=True)
    cp.pop('integration_workspace', None)
    cp.pop('candidate_validation_helpers', None)
    for key in ('native_regression_controls', 'library_regression_controls'):
        cp.pop(key, None)
        if config.get(key) is not None:
            cp[key] = config[key]
    for key, default in (('audit_threads', 24), ('stage_group_size', 1),
                         ('family_check_threads', retire_batch.FAMILY_CHECK_THREADS),
                         ('native_control_threads', retire_batch.DEFAULT_NATIVE_CONTROL_THREADS),
                         ('parse_screen_threads', retire_batch.DEFAULT_PARSE_SCREEN_THREADS),
                         ('early_default_print', False), ('early_retained_parse', False)):
        cp[key] = config.get(key, default)
    return cp


def stage_next(run):
    run = Path(run).resolve()
    config = r.read(run/'config.json')
    if config.get('speculative_staging') is not True or any((run/name).exists() for name in ('drain.json',)):
        return {'status': 'disabled'}
    found = in_flight(run)
    if found is None:
        return {'status': 'idle'}
    state, candidate, inputs = found
    selection = next_selection(run, config, state, candidate)
    if selection is None:
        return {'status': 'no-selection', 'candidate': candidate['commit']}
    spec, wanted = selection
    receipt_path = run/RECEIPT
    if receipt_path.is_file() and r.read(receipt_path).get('plan') == spec:
        return {'status': 'already-staged', 'candidate': candidate['commit']}
    if any(not isinstance(job, str) or not isinstance(attempt, str) for job, attempt in wanted):
        return {'status': 'malformed-plan', 'candidate': candidate['commit']}
    current = {row['id']: row['attempt_id'] for row in dispatch.Queue(run/'queue.sqlite').get_jobs('provisional-pass')}
    if any(current.get(job) != attempt for job, attempt in wanted):
        return {'status': 'selection-changed', 'candidate': candidate['commit']}
    items = dispatch.proposals(run, len(wanted), [job for job, _ in wanted])
    if [(item['job_id'], item['attempt_id']) for item in items] != wanted:
        return {'status': 'selection-changed', 'candidate': candidate['commit']}
    jobs = [{'job': job, 'attempt': attempt} for job, attempt in wanted]
    r.write(receipt_path, {'status': 'staging', 'pid': os.getpid(), 'checkpoint': candidate['commit'],
                           'plan': spec, 'jobs': jobs})
    cp = checkpoint_for(run, config, candidate, inputs)
    cp['early_default_print'] = cp['early_default_print'] and (
        not config.get('skip_mechanical_early_default_print', False)
        or any(item['kind'] != 'mechanical' for item in items))
    manifest = retire_batch.stage(run, cp, items, combine_selected=True)
    folder = Path(manifest['folder'])
    manifest['speculative'] = {'parent_candidate': candidate['commit'], 'parent_batch': candidate['id'],
                               'inputs': str(inputs), 'helpers': cp['helpers'], 'plan': spec}
    r.write(folder/'manifest.json', manifest)
    if manifest['status'] != 'provisional':
        _release(manifest)
        r.write(receipt_path, {'status': 'failed', 'checkpoint': candidate['commit'], 'plan': spec, 'jobs': jobs,
                               'manifest': manifest['id']})
        return {'status': 'stage-'+manifest['status'], 'manifest': manifest['id']}
    r.write(receipt_path, {'status': 'staged', 'manifest': manifest['id'], 'sha256': r.digest(folder/'manifest.json'),
                           'checkpoint': candidate['commit'], 'plan': spec, 'jobs': jobs})
    return {'status': 'staged', 'manifest': manifest['id'], 'candidate': candidate['commit']}


def ready(run, checkpoint, selected, *, wait_seconds=0, poll_seconds=10):
    """Return the staged batch for this exact accepted checkpoint and selection, if any.

    A stage of the same selection that is still running started before this
    checkpoint was accepted, so waiting for it is bounded by its head start.
    """
    path = Path(run)/RECEIPT
    deadline = time.monotonic()+wait_seconds
    while path.is_file():
        receipt = r.read(path)
        if (receipt.get('checkpoint') != checkpoint
                or [(row['job'], row['attempt']) for row in receipt.get('jobs', ())] != selected):
            return None
        if receipt.get('status') == 'staging' and _alive(receipt.get('pid')) and time.monotonic() < deadline:
            time.sleep(poll_seconds)
            continue
        folder = Path(run)/'.cache/batches'/str(receipt.get('manifest'))
        if (receipt.get('status') != 'staged' or not (folder/'manifest.json').is_file()
                or r.digest(folder/'manifest.json') != receipt.get('sha256')):
            return None
        return receipt['manifest']
    return None


def _release(manifest):
    source = Path(manifest['source'])
    if source.is_dir() and source.resolve() == (Path(manifest['folder'])/'source').resolve():
        subprocess.run(['git', '-C', str(source), 'worktree', 'remove', '--force', str(source)],
                       check=False, capture_output=True)


def sweep(run):
    """Release worktrees of speculative stages that can no longer be adopted."""
    run = Path(run).resolve()
    head = r.read(run/'latest-checkpoint.json')['commit']
    released = 0
    for path in (run/'.cache/batches').glob('*/manifest.json'):
        manifest = r.read(path)
        speculative = manifest.get('speculative')
        if (not speculative or manifest.get('adopted') or speculative['parent_candidate'] == head
                or not (path.parent/'source').is_dir()):
            continue
        if any(r.read(p).get('id') == speculative['parent_batch'] and not (p.parent/'validation.json').is_file()
               for p in [run/'.cache/batches'/speculative['parent_batch']/'manifest.json'] if p.is_file()):
            continue
        _release(manifest)
        released += 1
    return released


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    args = parser.parse_args()
    with r.locked(args.run/'speculative-stage.lock', blocking=False) as owned:
        if not owned:
            return
        result = stage_next(args.run)
        result['released'] = sweep(args.run)
        print(json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
