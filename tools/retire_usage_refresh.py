#!/usr/bin/env python3
"""Refresh attempt-scoped sanitized cost sidecars without changing queue results."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import tempfile
import uuid

import retire_metrics


def _hash(data):
    return hashlib.sha256(data).hexdigest()


def _canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def _read_bounded(path, limit):
    with path.open('rb') as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError('usage input exceeds size limit')
    return data


def _inputs(campaign, attempt, job, started_raw, result_raw, config):
    try:
        parsed = uuid.UUID(attempt)
        if parsed.version != 4 or attempt not in (str(parsed), parsed.hex):
            return None
        if not retire_metrics.TASK.fullmatch(job) or job in ('.', '..'):
            return None
        cache = campaign/'.cache'
        if cache.is_symlink() or not cache.resolve().is_relative_to(campaign):
            return None
        attempts = cache/'attempts'
        run = attempts/attempt
        if attempts.is_symlink() or run.is_symlink() or not run.resolve().is_relative_to(cache.resolve()):
            return None
        receipt = run/'workers'/job/'receipt.json'
        events = run/'workers'/job/'events.jsonl'
        # Never follow worker-directory symlinks into another attempt.
        if any(path.is_symlink() for path in (run/'workers', run/'workers'/job, receipt, events)):
            return None
        if not run.resolve().is_relative_to((campaign/'.cache/attempts').resolve()):
            return None
        if not receipt.resolve().is_relative_to(run.resolve()) or not events.resolve().is_relative_to(run.resolve()):
            return None
        started = json.loads(started_raw) if started_raw else {}
        receipt_bytes = _read_bounded(receipt, 1024 * 1024)
        event_bytes = _read_bounded(events, 64 * 1024 * 1024)
        data = json.loads(receipt_bytes)
        if (data.get('task') != job or data.get('status') not in ('completed', 'needs-review', 'failed', 'timeout')
                or type(data.get('pid')) is not int or data['pid'] <= 0
                or started.get('pid') != data['pid'] or started.get('receipt') != str(receipt)
                or type(data.get('finished_unix')) not in (int, float)):
            return None
        metrics = config['metrics']
        estimator = Path(metrics['usage_estimator'])
        estimator_hash = _hash(_read_bounded(estimator, 4 * 1024 * 1024))
        if estimator_hash != metrics['estimator_sha256']:
            return None
        identity = {'attempt_id': attempt, 'job_id': job, 'started_sha256': _hash(started_raw.encode()),
                    'result_sha256': _hash(result_raw.encode()), 'receipt_sha256': _hash(receipt_bytes),
                    'events_sha256': _hash(event_bytes), 'estimator_sha256': estimator_hash,
                    'pricing_sha256': _hash(_canonical(metrics['pricing']))}
        return run, data, identity
    except (OSError, ValueError, KeyError, TypeError, AttributeError, OverflowError):
        return None


def verified_sidecar(campaign, attempt, job, started_raw, result_raw, config):
    """Return matching sanitized usage or None; never read any model rollout."""
    campaign = Path(campaign).resolve()
    try:
        path = campaign/'.cache/usage-receipts'/f'{attempt}.json'
        if path.is_symlink() or not path.resolve().is_relative_to((campaign/'.cache').resolve()):
            return None
        record = json.loads(path.read_text())
        inputs = _inputs(campaign, attempt, job, started_raw, result_raw, config)
        if not inputs or record.get('identity') != inputs[2] or record.get('schema') != 1:
            return None
        usage = record.get('usage')
        return usage if isinstance(usage, dict) else None
    except (OSError, ValueError, TypeError, KeyError):
        return None


def refresh(campaign):
    """Refresh terminal author attempts, including prior attempts after retry."""
    campaign = Path(campaign).resolve()
    config = json.loads((campaign/'config.json').read_text())
    db = sqlite3.connect((campaign/'queue.sqlite').as_uri()+'?mode=ro', uri=True)
    try:
        db.execute('PRAGMA query_only=ON')
        rows = db.execute('SELECT a.id,a.job_id,a.started,a.result,j.payload FROM attempts a '
                          'JOIN jobs j ON j.id=a.job_id WHERE a.result IS NOT NULL').fetchall()
    finally:
        db.close()
    folder = campaign/'.cache/usage-receipts'
    if ((campaign/'.cache').is_symlink() or not (campaign/'.cache').resolve().is_relative_to(campaign)
            or folder.is_symlink() or not folder.resolve().is_relative_to((campaign/'.cache').resolve())):
        raise ValueError('usage receipt directory escapes campaign cache')
    folder.mkdir(parents=True, exist_ok=True)
    counts = {'examined': 0, 'written': 0, 'unchanged': 0, 'unavailable': 0, 'original': 0}
    for attempt, job, started_raw, result_raw, payload in rows:
        if json.loads(payload).get('kind') != 'author':
            continue
        counts['examined'] += 1
        result = json.loads(result_raw)
        evidence = result.get('evidence')
        if isinstance(evidence, dict) and isinstance(evidence.get('usage'), dict):
            counts['original'] += 1
            continue
        inputs = _inputs(campaign, attempt, job, started_raw, result_raw, config)
        if not inputs:
            counts['unavailable'] += 1
            continue
        run, receipt, identity = inputs
        try:
            usage = retire_metrics.collect(run, job, receipt, config['metrics'])
            if _inputs(campaign, attempt, job, started_raw, result_raw, config) != inputs:
                raise ValueError('inputs changed while collecting')
            encoded = _canonical({'schema': 1, 'identity': identity, 'usage': usage}) + b'\n'
            target = folder/f'{attempt}.json'
            if target.is_symlink():
                raise ValueError('sidecar must not be a symlink')
            if target.exists() and target.read_bytes() == encoded:
                counts['unchanged'] += 1
                continue
            fd, name = tempfile.mkstemp(prefix='.usage-', dir=folder)
            try:
                with os.fdopen(fd, 'wb') as output:
                    output.write(encoded)
                os.replace(name, target)
            finally:
                Path(name).unlink(missing_ok=True)
            counts['written'] += 1
        except (OSError, ValueError, KeyError, TypeError, AttributeError, OverflowError):
            counts['unavailable'] += 1
    return counts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    args = parser.parse_args()
    print(json.dumps(refresh(args.campaign), sort_keys=True))


if __name__ == '__main__':
    main()
