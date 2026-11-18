#!/usr/bin/env python3
"""Reconcile finished worker worktrees through the normal verified archive flow."""
import argparse
from contextlib import closing
import json
from pathlib import Path
import re
import sqlite3
import subprocess
import tarfile

import retire as r
from retire_queue import Queue
from retire_retention import archive_checkout

_ATTEMPT = re.compile(r'[0-9a-f]{32}|[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}')


def _candidates(campaign):
    workspace = campaign/'workspaces'
    database = campaign/'queue.sqlite'
    if not database.is_file() or database.is_symlink() or workspace.resolve() != workspace:
        raise ValueError('campaign queue or workspace is unavailable')
    # A prepared campaign can have a queue before it creates its first checkout.
    if not workspace.exists():
        return []
    checked_out = {p.name for p in workspace.iterdir()
                   if _ATTEMPT.fullmatch(p.name) and p.is_dir() and not p.is_symlink()}
    items = sorted(checked_out)
    result = []
    with closing(sqlite3.connect(database.as_uri()+'?mode=ro', uri=True)) as db:
        for offset in range(0, len(items), 256):
            group = items[offset:offset+256]
            placeholders = ','.join('?' for _ in group)
            result.extend(db.execute('SELECT id,job_id,started,result FROM attempts '
                                     f'WHERE result IS NOT NULL AND id IN ({placeholders})', group))
    return result


def _identity(campaign, attempt, job, started, result):
    owner = json.loads(started or '{}')
    pid = owner.get('pid')
    if type(pid) is not int or pid <= 0:
        raise ValueError('the finished worker has no recorded process identity')
    Queue._require_absent_pid(pid)
    evidence = json.loads(result)['evidence']
    run = campaign/'.cache/attempts'/attempt
    receipt = campaign/'.cache/receipts'/(attempt+'.json')
    checkout = campaign/'workspaces'/attempt
    if (not receipt.is_file() or receipt.resolve() != receipt or not run.is_dir() or run.resolve() != run
            or checkout.resolve() != checkout or evidence.get('receipt') != str(receipt)
            or evidence.get('run') != str(run) or evidence.get('task') != job):
        raise ValueError('finished attempt does not own the archived workspace')
    return run, receipt


def clean(campaign, apply):
    candidates = _candidates(campaign)
    result = {'applied': apply, 'eligible': [], 'removed': [], 'retained': []}
    if not candidates:
        return result
    queue = Queue(campaign/'queue.sqlite')
    for attempt, job, started, report in candidates:
        try:
            with queue._retry_lease(job):
                run, receipt = _identity(campaign, attempt, job, started, report)
                result['eligible'].append(attempt)
                if apply:
                    archive = archive_checkout(run, job, receipt)
                    r.write(run/'archive.json', archive)
                    result['removed'].append(attempt)
        except (OSError, ValueError, KeyError, TypeError, subprocess.CalledProcessError,
                tarfile.TarError) as error:
            # Preserve the checkout if its owner or the recorded archive differs.
            result['retained'].append({'attempt': attempt, 'reason': str(error)})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    campaign = args.campaign.resolve()
    if not args.apply:
        print(json.dumps(clean(campaign, False), indent=2))
        return
    folder = campaign/'.cache/storage'
    if folder.resolve() != folder:
        raise ValueError('unsafe storage state path')
    folder.mkdir(parents=True, exist_ok=True)
    with r.locked(folder/'maintenance.lock', blocking=False) as acquired:
        if not acquired:
            return
        result = clean(campaign, True)
        r.write(folder/'worktree-cleanup.json', result)
        print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
