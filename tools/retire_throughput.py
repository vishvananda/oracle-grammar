#!/usr/bin/env python3
"""Report durable retirement throughput and completed integration time."""
import argparse
from collections import Counter
from contextlib import closing
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sqlite3
import subprocess
import time

import retire as r

WINDOWS = (3, 6, 24)
BATCH_ID = re.compile(r'[0-9a-f]{32}')


def _snapshot(run, earliest):
    with closing(sqlite3.connect((run/'queue.sqlite').as_uri()+'?mode=ro', uri=True)) as db:
        db.execute('BEGIN')
        checkpoint = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        counts = dict(db.execute("""SELECT j.status, count(r.rule) FROM rules r
            JOIN jobs j ON r.job_id=j.id GROUP BY j.status"""))
        accepted = dict(db.execute("""SELECT j.id, count(r.rule) FROM rules r
            JOIN jobs j ON r.job_id=j.id WHERE j.status='accepted' GROUP BY j.id"""))
        events = db.execute("""SELECT kind, data, unixepoch(at) FROM events
            WHERE kind IN ('adjudicated', 'diagnostic-recovery') AND at>=datetime(?, 'unixepoch')
            ORDER BY seq""", (earliest,)).fetchall()
    return checkpoint[0] if checkpoint else None, counts, accepted, events


def _decisions(events, accepted):
    batches, accepted_rules = {}, []
    for kind, encoded, at in events:
        event = json.loads(encoded)
        evidence = event['evidence']
        passed = kind == 'diagnostic-recovery' or event['status'] == 'accepted'
        if passed:
            accepted_rules.append((at, sum(accepted[job] for job in event['jobs'])))
        ident = evidence.get('manifest')
        if not isinstance(ident, str) or BATCH_ID.fullmatch(ident) is None:
            continue
        validation = evidence.get('validation') or {}
        losses = validation.get('full_checks', {}).get('failures')
        reason = 'accepted' if passed else validation.get('reason', evidence.get('reason', 'rejected'))
        if not passed and losses and all(item.get('lost') == ['exact_default'] for item in losses):
            reason = 'exact default wording lost'
        batches[ident] = (at, reason)
    return batches, accepted_rules


def _integration_spans(run, batches):
    spans, missing = [], 0
    for ident, (end, outcome) in batches.items():
        folder = run/'.cache/batches'/ident
        try:
            manifest = r.read(folder/'manifest.json')
            start = manifest.get('started_unix')
            if start is None:
                start = int(subprocess.check_output(['stat', '-c', '%W', folder], timeout=3))
            if manifest['id'] != ident or not 0 < start <= end+1:
                raise ValueError('batch start could not be verified')
        except (OSError, ValueError, KeyError, subprocess.SubprocessError):
            missing += 1
            continue
        spans.append((start, end, outcome))
    return spans, missing


def _hours(spans, since):
    """Count the union of timed intervals, clipping intervals to the window."""
    end, seconds = since, 0
    for start, finish, _ in sorted(spans):
        if finish >= since:
            seconds += max(0, finish-max(start, end))
            end = max(end, finish)
    return seconds/3600


def report(run, now):
    checkpoint, counts, accepted, events = _snapshot(run, now-max(WINDOWS)*3600)
    batches, decisions = _decisions(events, accepted)
    spans, missing = _integration_spans(run, batches)
    windows = {}
    for width in WINDOWS:
        since = now-width*3600
        rules = sum(count for at, count in decisions if at >= since)
        hours = _hours(spans, since)
        outcomes = Counter(outcome for at, outcome in batches.values() if at >= since)
        windows[str(width)] = {
            'accepted_rules': rules, 'accepted_per_wall_hour': round(rules/width, 2),
            'completed_integration_hours': round(hours, 3),
            'accepted_per_completed_integration_hour': round(rules/hours, 2) if hours else None,
            'batch_outcomes': dict(outcomes),
        }
    return {'sampled_at': datetime.fromtimestamp(now, timezone.utc).isoformat(),
            'checkpoint': checkpoint, 'rules_by_status': counts,
            'completed_batches_missing_timing': missing, 'windows_hours': windows}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    r.write(args.output, report(args.campaign.resolve(), time.time()))


if __name__ == '__main__':
    main()
