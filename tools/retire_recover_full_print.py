#!/usr/bin/env python3
"""Recover bounded parse, print or library batch rejections in the background."""
import argparse
import json
from pathlib import Path
import subprocess
import time

import retire as r
import retire_batch as batches
import retire_library_isolate as library
import retire_parse_isolate as isolation
import retire_tasks as tasks
from retire_native_readmission import _allowed
from retire_parse_readmission import pending_failed_batches
from retire_queue import Queue


def _profile_names(spec):
    path = Path(spec['path'])
    r.require(path.is_absolute() and path.is_file() and not path.is_symlink()
              and path.stat().st_size <= 1024*1024 and r.digest(path) == spec['sha256'],
              'retained profile changed')
    names = r.read(path)['cards']
    r.require(isinstance(names, list) and 1 <= len(names) <= tasks.MAX_BASE_COHORT_NAMES
              and all(isinstance(name, str) and name for name in names)
              and len(set(names)) == len(names), 'retained profile names are invalid')
    return set(names)


def _retain_recovery_faces(campaign, folder, result):
    """Add the isolated population after checking the accepted baseline."""
    config_path = campaign/'config.json'
    if not config_path.is_file() or 'native_regression_controls' not in r.read(config_path):
        return
    proof = result['evidence']['isolation']
    path = Path(proof['path'])
    r.require(path.is_relative_to(folder) and path.resolve() == path and r.digest(path) == proof['sha256'],
              'isolation evidence changed')
    names = _profile_names(r.read(path)['profile'])
    queue = Queue(campaign/'queue.sqlite')
    for attempt in range(3):
        before = r.read(config_path)['native_regression_controls']
        present = _profile_names(before)
        addition = names - present
        if not addition:
            return
        head = queue.progress()['checkpoint']
        cp = r.read(campaign/'checkpoints'/f'{head}.json')
        grammar = Path(cp['source'])/'grammar'
        inputs = Path(cp['inputs'])
        accepted = r.tree(grammar)
        r.require(cp['commit'] == head and bool(accepted) and accepted == r.tree(inputs/'frozen/grammar'),
                  'accepted grammar changed')
        tasks.close_cohort(inputs, {'names': sorted(present | addition)})
        new_names = folder/f'retained-additions-{attempt}.json'
        r.write(new_names, {'cards': sorted(addition)})
        new_names.chmod(0o444)
        cp.update(native_regression_controls=isolation._spec(new_names),
                  native_control_threads=2, background_cpu=True)
        control = batches._control_setup(cp)
        checked = batches._control_check(cp, control, grammar, folder/f'retained-baseline-{attempt}')
        r.require(checked['failures'] == [] and r.tree(grammar) == accepted,
                  'accepted baseline did not preserve the recovered controls')
        with r.locked(campaign/'campaign-tuning.lock', blocking=True):
            config = r.read(config_path)
            if config['native_regression_controls'] != before or queue.progress()['checkpoint'] != head:
                continue
            profile = folder/'retained-profile.json'
            r.write(profile, {'cards': sorted(present | addition)})
            profile.chmod(0o444)
            config['native_regression_controls'] = isolation._spec(profile)
            r.write(config_path, config)
            return {'checkpoint': head, 'added': sorted(addition), 'previous': before,
                    'profile': config['native_regression_controls']}
    raise ValueError('the accepted checkpoint or retained profile changed during validation')


def recover(campaign, output, batch, max_trials, *, apply, mode='print'):
    folder = output/batch
    try:
        path = folder/'proof'
        if mode == 'library':
            result = library.isolate(campaign, batch, path, max_trials)
            if apply:
                library.readmit_result(campaign, path, result)
        else:
            r.require(mode in ('print', 'parse'), 'unsupported batch recovery mode')
            full_check = mode == 'print'
            retry_trials = min(max_trials, 96-max_trials)
            result = isolation.isolate(campaign, batch, path, max_trials, full_check=full_check,
                                       witness_retry=retry_trials if retry_trials >= 3 else 0)
            if apply:
                try:
                    retained = _retain_recovery_faces(campaign, folder, result)
                except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
                    retained = {'deferred': str(error)}
                if retained:
                    r.write(folder/'retained-control-extension.json', retained)
                isolation.readmit_result(campaign, path, result, full_check=full_check)
        receipt = {'status': 'readmitted' if apply else 'proved', 'rules': result['rules'],
                   'jobs': len(result['jobs']), 'proof': str(path/'result.json')}
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        receipt = {'status': 'needs-review', 'reason': str(error)}
    folder.mkdir(exist_ok=True)
    r.write(folder/'outcome.json', {'batch': batch, **receipt})
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--watch', action='store_true')
    parser.add_argument('--apply', action='store_true')
    parser.add_argument('--include-parse', action='store_true', help='also isolate localized parse regressions')
    parser.add_argument('--include-library', action='store_true', help='also isolate exact library test failures')
    parser.add_argument('--max-trials', type=int, default=48)
    args = parser.parse_args()
    campaign, output = args.campaign.resolve(), args.output.resolve()
    if not 3 <= args.max_trials <= 96 or not _allowed(campaign, output):
        parser.error('use a private scratch output and 3–96 isolation trials')
    output.mkdir(parents=True, exist_ok=True)
    with r.locked(campaign/'full-print-recovery.lock', blocking=False) as owned:
        r.require(owned, 'another full print recovery process owns the campaign')
        r.write(campaign/'full-print-recovery.json', {'output': str(output) if args.watch and args.apply else None,
                                                    'include_parse': args.include_parse,
                                                    'include_library': args.include_library})
        while not (campaign/'drain.json').exists():
            for batch, mode in pending_failed_batches(campaign, output, include_parse=args.include_parse,
                                                      include_library=args.include_library):
                if (campaign/'drain.json').exists():
                    return
                with r.locked(output/f'{batch}.lock'):
                    receipt = recover(campaign, output, batch, args.max_trials, apply=args.apply, mode=mode)
                print(json.dumps({'batch': batch, **receipt}), flush=True)
            if not args.watch:
                return
            time.sleep(10)


if __name__ == '__main__':
    main()
