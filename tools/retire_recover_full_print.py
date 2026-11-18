#!/usr/bin/env python3
"""Recover small, complete parse or print batch rejections in the background."""
import argparse
import json
from pathlib import Path
import subprocess
import time

import retire as r
import retire_parse_isolate as isolation
from retire_native_readmission import _allowed
from retire_parse_readmission import pending_failed_batches


def recover(campaign, output, batch, max_trials, *, apply, full_check=True):
    folder = output/batch
    try:
        result = isolation.isolate(campaign, batch, folder/'proof', max_trials, full_check=full_check)
        if apply:
            isolation.readmit_result(campaign, folder/'proof', result, full_check=full_check)
        receipt = {'status': 'readmitted' if apply else 'proved', 'rules': result['rules'],
                   'jobs': len(result['jobs']), 'proof': str(folder/'proof/result.json')}
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
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
    parser.add_argument('--include-parse', action='store_true', help='also isolate small parse additions')
    parser.add_argument('--max-trials', type=int, default=48)
    args = parser.parse_args()
    campaign, output = args.campaign.resolve(), args.output.resolve()
    if not 3 <= args.max_trials <= 96 or not _allowed(campaign, output):
        parser.error('use a private scratch output and 3–96 isolation trials')
    output.mkdir(parents=True, exist_ok=True)
    with r.locked(campaign/'full-print-recovery.lock', blocking=False) as owned:
        r.require(owned, 'another full print recovery process owns the campaign')
        r.write(campaign/'full-print-recovery.json', {'output': str(output) if args.watch and args.apply else None,
                                                    'include_parse': args.include_parse})
        while not (campaign/'drain.json').exists():
            for batch, full_check in pending_failed_batches(campaign, output, include_parse=args.include_parse):
                if (campaign/'drain.json').exists():
                    return
                with r.locked(output/f'{batch}.lock'):
                    receipt = recover(campaign, output, batch, args.max_trials, apply=args.apply,
                                      full_check=full_check)
                print(json.dumps({'batch': batch, **receipt}), flush=True)
            if not args.watch:
                return
            time.sleep(10)


if __name__ == '__main__':
    main()
