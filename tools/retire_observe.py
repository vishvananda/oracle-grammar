#!/usr/bin/env python3
"""Read-only, aggregate campaign observation; never open worker rollouts."""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import statistics
import tempfile

from retire_usage_refresh import verified_sidecar, verified_taint

TOKENS = ('input_tokens', 'cached_input_tokens', 'cache_write_input_tokens',
          'output_tokens', 'reasoning_output_tokens')


def _duration(start, end):
    try:
        first = datetime.fromisoformat(start).replace(tzinfo=timezone.utc)
        last = datetime.fromisoformat(end).replace(tzinfo=timezone.utc)
        seconds = (last-first).total_seconds()
        return seconds if seconds >= 0 else None
    except (ValueError, TypeError):
        return None


def _median(values):
    return statistics.median(values) if values else None


def _number(value):
    return value if type(value) in (int, float) and 0 <= value < float('inf') else None


def _disk(path):
    usage = shutil.disk_usage(path)
    return {'total_bytes': usage.total, 'free_bytes': usage.free}


def _failure_category(reason):
    text = reason.lower().replace('-', '_').replace(' ', '_') if isinstance(reason, str) else ''
    for category, labels in (
            ('exact_keyed', ('exact_key', 'keyed_exact')),
            ('exact_default', ('exact_default', 'default_exact')),
            ('round_trip', ('round_trip', 'semantic_reparse')),
            ('native', ('native', 'reading_preservation')),
            ('parse', ('parse', 'recognition')),
            ('operational', ('operational', 'timeout', 'permission_denied'))):
        if any(label in text for label in labels):
            return category
    return 'other'


def _verified_unstarted(event):
    evidence = event.get('evidence')
    if not isinstance(evidence, dict) or evidence.get('launch_not_attempted') is not True:
        return False
    terminal = evidence.get('controller_terminal')
    return (isinstance(terminal, dict) and isinstance(terminal.get('unit'), str) and bool(terminal['unit'])
            and isinstance(terminal.get('invocation'), str) and bool(terminal['invocation'])
            and isinstance(terminal.get('verification'), dict) and bool(terminal['verification']))


def _audit_command_times(path):
    try:
        if path != path.resolve() or not path.is_file() or path.stat().st_size > 8192:
            return None
        with path.open() as stream:
            content = stream.read(8193)
        if len(content) > 8192:
            return None
        rows = [json.loads(line) for line in content.splitlines()]
        if ([(row['program'], row['returncode']) for row in rows]
                != [('score', 0), ('validate-readings', 0)]):
            return None
        times = [_number(row['wall_seconds']) for row in rows]
        return times if all(time is not None for time in times) else None
    except (OSError, KeyError, TypeError, ValueError):
        return None


def _audit_timings(campaign, checkpoints):
    groups = defaultdict(list)
    for commit, raw in checkpoints:
        receipt = json.loads(raw)
        if not isinstance(receipt, dict):
            continue
        batch, validation = receipt.get('manifest'), receipt.get('validation')
        if (not isinstance(batch, str) or not re.fullmatch(r'[0-9a-f]{32}', batch)
                or not isinstance(validation, dict) or validation.get('status') != 'passed'
                or validation.get('candidate') != commit):
            continue
        screen, full = (validation.get(key, {}) for key in ('parse_screen', 'full_checks'))
        if not isinstance(screen, dict) or not isinstance(full, dict):
            continue
        population = full.get('faces')
        parse_threads, full_threads = (validation.get(key) for key in ('parse_screen_threads', 'audit_threads'))
        if (screen.get('status') != 'passed' or screen.get('faces') != population or full.get('failures') != []
                or any(type(value) is not int or value <= 0 for value in (population, parse_threads, full_threads))):
            continue
        folder = campaign/'.cache/batches'/batch
        parse = _audit_command_times(folder/'parse-screen/commands.jsonl')
        rendered = _audit_command_times(folder/'full-checks/commands.jsonl')
        if parse is not None and rendered is not None:
            groups[population, parse_threads, full_threads].append(parse + rendered)
    keys = ('parse_score_seconds', 'parse_native_seconds', 'render_round_trip_score_seconds', 'full_native_seconds')
    return [{'population': population, 'parse_threads': parse_threads, 'full_threads': full_threads,
             'samples': len(samples), 'wall_medians': dict(zip(keys, map(statistics.median, zip(*samples)), strict=True))}
            for (population, parse_threads, full_threads), samples in sorted(groups.items())]


def snapshot(campaign):
    """Take one SQLite read transaction; report aggregates without source text."""
    campaign = Path(campaign).resolve()
    config = json.loads((campaign/'config.json').read_text())
    taint_path = campaign/'.cache/usage-taints.json'
    if taint_path.is_symlink():
        raise ValueError('usage taint manifest must not be a symlink')
    if taint_path.exists():
        with taint_path.open('rb') as stream:
            data = stream.read(8 * 1024 * 1024 + 1)
        if len(data) > 8 * 1024 * 1024:
            raise ValueError('usage taint manifest exceeds size limit')
        document = json.loads(data)
        if document.get('schema') != 1 or not isinstance(document.get('attempts'), dict):
            raise ValueError('invalid usage taint manifest')
        usage_taints = document['attempts']
    else:
        usage_taints = {}
    uri = (campaign/'queue.sqlite').as_uri() + '?mode=ro'
    db = sqlite3.connect(uri, uri=True, isolation_level=None)
    try:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        jobs = list(db.execute('SELECT id,payload,status FROM jobs'))
        rules = list(db.execute('SELECT j.status,j.payload,count(*) FROM rules r '
                                'JOIN jobs j ON j.id=r.job_id GROUP BY j.id'))
        attempts = list(db.execute('SELECT id,job_id,started,result FROM attempts'))
        events = list(db.execute("SELECT kind,data,at FROM events WHERE kind IN "
                                 "('claimed','finished','retry-tainted','retry-unstarted') ORDER BY seq"))
        checkpoints = list(db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 16'))
        db.commit()
    finally:
        db.close()
    job_counts, rule_counts = Counter(), Counter()
    kinds = {}
    for job_id, payload, status in jobs:
        kind = json.loads(payload).get('kind') or 'unknown'
        kinds[job_id] = kind
        if status != 'superseded':
            job_counts[(status, kind)] += 1
    for status, payload, count in rules:
        rule_counts[(status, json.loads(payload).get('kind') or 'unknown')] += count
    times, tainted, unstarted = defaultdict(dict), {}, set()
    for kind, data, at in events:
        event = json.loads(data)
        attempt = event.get('attempt_id')
        if kind in ('claimed', 'finished'):
            times[attempt][kind] = at
        else:
            tainted[attempt] = kind
            if kind == 'retry-unstarted' and _verified_unstarted(event):
                unstarted.add(attempt)
    groups = defaultdict(lambda: {'attempts': 0, 'usage_status': Counter(), 'tokens': Counter(),
                                  'known_estimated_usd': 0.0, 'unknown_cost': 0,
                                  'model_seconds': [], 'slot_seconds': [], 'integrity_status': Counter()})
    failures = Counter()
    for attempt_id, job_id, started_raw, raw in attempts:
        result = json.loads(raw) if raw else None
        state = result.get('status', 'unknown') if isinstance(result, dict) else 'running'
        evidence = result.get('evidence', {}) if isinstance(result, dict) else {}
        usage = evidence.get('usage') if isinstance(evidence, dict) else None
        if not isinstance(usage, dict) and raw and started_raw and kinds.get(job_id) == 'author':
            usage = verified_sidecar(campaign, attempt_id, job_id, started_raw, raw, config)
        usage = usage if isinstance(usage, dict) else {}
        integrity = verified_taint(campaign, attempt_id, job_id, started_raw, usage_taints)
        if integrity:
            usage = {**usage, 'status': 'partial'}
        cohort = tainted.get(attempt_id, 'ordinary')
        no_model = not usage and (kinds.get(job_id) == 'mechanical' or attempt_id in unstarted)
        if no_model:
            usage = {'status': 'not-applicable', 'estimated_usd': 0.0}
        model = 'none' if no_model else usage.get('model') if isinstance(usage.get('model'), str) else 'unknown'
        key = (cohort, kinds.get(job_id, 'unknown'), state, model)
        group = groups[key]
        group['attempts'] += 1
        if integrity:
            group['integrity_status'][integrity] += 1
        group['usage_status'][usage.get('status') or 'unavailable'] += 1
        cost = _number(usage.get('estimated_usd'))
        if cost is None or usage.get('status') not in ('recorded', 'not-applicable'):
            group['unknown_cost'] += 1
        if cost is not None:
            group['known_estimated_usd'] += cost
        tokens = usage.get('tokens')
        if isinstance(tokens, dict):
            for token in TOKENS:
                value = tokens.get(token)
                if type(value) is int and value >= 0:
                    group['tokens'][token] += value
        elapsed = _number(usage.get('elapsed_seconds'))
        if elapsed is not None:
            group['model_seconds'].append(elapsed)
        timing = times.get(attempt_id, {})
        duration = _duration(timing.get('claimed'), timing.get('finished'))
        if duration is not None:
            group['slot_seconds'].append(duration)
        if result:
            for outcome in result.get('outcomes', {}).values():
                if isinstance(outcome, dict) and outcome.get('status') == 'needs-review':
                    reason = outcome.get('reason')
                    digest = hashlib.sha256(reason.encode()).hexdigest() if isinstance(reason, str) else None
                    failures[(state, _failure_category(reason), digest)] += 1
    metrics = []
    for (cohort, kind, status, model), group in sorted(groups.items()):
        metrics.append({'cohort': cohort, 'kind': kind, 'status': status, 'model': model,
                        'attempts': group['attempts'], 'usage_status': dict(group['usage_status']),
                        'usage_integrity': dict(group['integrity_status']),
                        'tokens': {token: group['tokens'][token] for token in TOKENS},
                        'known_estimated_usd': group['known_estimated_usd'],
                        'estimated_usd': group['known_estimated_usd'] if not group['unknown_cost'] else None,
                        'unknown_cost': group['unknown_cost'],
                        'model_wall_median_seconds': _median(group['model_seconds']),
                        'model_wall_samples': len(group['model_seconds']),
                        'claim_finish_median_seconds': _median(group['slot_seconds']),
                        'claim_finish_samples': len(group['slot_seconds'])})
    def counts(values):
        return [{'status': status, 'kind': kind, 'count': count}
                for (status, kind), count in sorted(values.items())]
    unknown_cost = sum(group['unknown_cost'] for group in metrics)
    known_cost = sum(group['known_estimated_usd'] for group in metrics)
    return {'jobs': counts(job_counts), 'rules': counts(rule_counts), 'attempts': metrics,
            'cost_total': {'known_estimated_usd': known_cost, 'unknown_attempts': unknown_cost,
                           'estimated_usd': None if unknown_cost else known_cost},
            'failure_categories': [{'attempt_status': state, 'category': category,
                                    'reason_sha256': digest, 'rules': count}
                                   for (state, category, digest), count in sorted(failures.items(), key=lambda x: str(x[0]))],
            'accepted_audit_timings': _audit_timings(campaign, checkpoints),
            'filesystem': {'campaign': _disk(campaign), 'main': _disk(Path(config['main_repo']))},
            'cost_basis': 'Standard API equivalent estimates, not subscription billing'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = snapshot(args.campaign)
    encoded = json.dumps(result, indent=2, sort_keys=True, allow_nan=False)+'\n'
    if args.output:
        campaign = args.campaign.resolve()
        cache_path = campaign/'.cache'
        cache = cache_path.resolve()
        target = args.output.resolve()
        if (cache_path.is_symlink() or not cache.is_relative_to(campaign)
                or not target.is_relative_to(cache) or target == cache or not target.parent.is_dir()):
            raise ValueError('output must be a file inside campaign .cache')
        fd, name = tempfile.mkstemp(prefix='.observe-', dir=target.parent)
        try:
            with os.fdopen(fd, 'w') as stream:
                stream.write(encoded)
            os.replace(name, target)
        finally:
            Path(name).unlink(missing_ok=True)
    else:
        print(encoded, end='')


if __name__ == '__main__':
    main()
