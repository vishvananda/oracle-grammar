#!/usr/bin/env python3
"""Recover small, complete print-only batch rejections in the background."""
import argparse
from collections import defaultdict
from contextlib import closing
import json
from pathlib import Path
import re
import sqlite3
import subprocess
import time

import retire as r
import retire_parse_isolate as isolation
from retire_native_readmission import _allowed


REJECTION = 'full allowed-population checks failed'


def _focused_print_losses(failures):
    return (isinstance(failures, list) and 1 <= len(failures) <= 16
            and all(isinstance(row, dict) and set(row) == {'face', 'lost'}
                    and isinstance(row['face'], str) and row['face']
                    and row['lost'] == ['exact_default'] for row in failures))


def pending_batches(campaign, output):
    """Select an untouched current-attempt group. The readmission verifier owns proof."""
    groups = defaultdict(set)
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        for job, attempt, batch in db.execute("""
                SELECT id, attempt_id, json_extract(adjudication,'$.evidence.manifest')
                FROM jobs WHERE status='integration-rejected'
                AND json_extract(adjudication,'$.evidence.validation.reason')=?
                """, (REJECTION,)):
            if isinstance(batch, str) and re.fullmatch(r'[0-9a-f]{32}', batch):
                groups[batch].add((job, attempt))
    ready = []
    for batch, current in groups.items():
        if not 2 <= len(current) <= 96 or (output/batch).exists():
            continue
        folder = campaign/'.cache/batches'/batch
        try:
            manifest = r.read(folder/'manifest.json')
            validation = r.read(folder/'validation.json')
            included = [job for job in manifest['jobs'] if job['status'] == 'included']
            original = {(job['job_id'], job['attempt_id']) for job in included}
            if (manifest['id'] == batch and current == original and len(included) == len(original)
                    and validation['reason'] == REJECTION and validation['status'] == 'failed'
                    and validation['scope'] == 'combined-batch'
                    and _focused_print_losses(validation['full_checks']['failures'])):
                ready.append((folder.stat().st_mtime_ns, batch))
        except (OSError, KeyError, ValueError, TypeError):
            # Missing, incomplete and legacy receipts stay available for review.
            continue
    return [batch for _, batch in sorted(ready)]


def recover(campaign, output, batch, max_trials, *, apply):
    folder = output/batch
    try:
        result = isolation.isolate(campaign, batch, folder/'proof', max_trials, full_check=True)
        if apply:
            isolation.readmit_result(campaign, folder/'proof', result, full_check=True)
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
    parser.add_argument('--max-trials', type=int, default=48)
    args = parser.parse_args()
    campaign, output = args.campaign.resolve(), args.output.resolve()
    if not 3 <= args.max_trials <= 96 or not _allowed(campaign, output):
        parser.error('use a private scratch output and 3–96 isolation trials')
    output.mkdir(parents=True, exist_ok=True)
    with r.locked(campaign/'full-print-recovery.lock', blocking=False) as owned:
        r.require(owned, 'another full print recovery process owns the campaign')
        while not (campaign/'drain.json').exists():
            for batch in pending_batches(campaign, output):
                if (campaign/'drain.json').exists():
                    return
                receipt = recover(campaign, output, batch, args.max_trials, apply=args.apply)
                print(json.dumps({'batch': batch, **receipt}), flush=True)
            if not args.watch:
                return
            time.sleep(10)


if __name__ == '__main__':
    main()
