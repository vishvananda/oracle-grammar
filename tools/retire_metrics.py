"""Attempt-scoped, sanitized Standard API-equivalent usage estimates.

The configured rollout estimator owns request deduplication and context bands.
Only the UUID assigned by this attempt's CLI events may be opened.
"""
from collections import defaultdict
from datetime import datetime, timedelta, timezone
import importlib.util
import json
from pathlib import Path
import re
import sys

UUID = re.compile(r'[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}')
TASK = re.compile(r'[A-Za-z0-9][A-Za-z0-9._-]{0,99}')
TOKENS = ('input_tokens', 'cached_input_tokens', 'cache_write_input_tokens',
          'output_tokens', 'reasoning_output_tokens')


def _events(path):
    try:
        with path.open() as stream:
            for line in stream:
                try:
                    event = json.loads(line)
                except ValueError:
                    continue
                if isinstance(event, dict):
                    yield event
    except FileNotFoundError:
        return


def _thread(events):
    ids = {e.get('thread_id') for e in events if e.get('type') == 'thread.started'}
    if len(ids) != 1:
        return None
    value = next(iter(ids))
    return value if isinstance(value, str) and UUID.fullmatch(value) else None


def _rollout(root, started, thread):
    date = datetime.fromtimestamp(started, timezone.utc).date()
    paths = []
    for offset in (-1, 0, 1):
        day = date + timedelta(days=offset)
        folder = root / day.strftime('%Y/%m/%d')
        paths.extend(path for path in folder.glob(f'*-{thread}.jsonl')
                     if not path.is_symlink() and path.is_file())
    if len(paths) > 1:
        raise ValueError('multiple rollouts for assigned thread')
    return paths[0] if paths else None


def _estimator(config):
    path = Path(config['usage_estimator'])
    spec = importlib.util.spec_from_file_location('_retirement_usage_estimator', path)
    if spec is None or spec.loader is None:
        raise ValueError('cannot load usage estimator')
    module = importlib.util.module_from_spec(spec)
    previous = sys.dont_write_bytecode
    try:
        sys.dont_write_bytecode = True
        spec.loader.exec_module(module)
    finally:
        sys.dont_write_bytecode = previous
    pricing = config['pricing']
    # Only explicitly configured prices may appear in this campaign's receipts.
    module.RATES.clear()
    module.RATES.update({name: tuple(rates) for name, rates in pricing['rates'].items()})
    module.SOURCES['codex-rollout'] = (pricing['version'], pricing['source'], pricing['basis'])
    return module


def _tokens(value):
    if not isinstance(value, dict) or any(k not in value for k in ('input_tokens', 'output_tokens')):
        return None
    result = {key: value.get(key, 0) for key in TOKENS}
    if any(type(n) is not int or n < 0 for n in result.values()):
        return None
    if (result['cached_input_tokens'] + result['cache_write_input_tokens'] > result['input_tokens']
            or result['reasoning_output_tokens'] > result['output_tokens']):
        return None
    return result


def collect(run: Path, taskid: str, receipt: dict, config: dict) -> dict:
    """Collect only this attempt's assigned session; never return source text or paths."""
    if not isinstance(taskid, str) or not TASK.fullmatch(taskid) or taskid in ('.', '..'):
        raise ValueError('invalid task ID')
    events = list(_events(Path(run) / 'workers' / taskid / 'events.jsonl'))
    thread = _thread(events)
    result = {'model': receipt.get('model'), 'effort': receipt.get('effort'),
              'elapsed_seconds': receipt.get('wall_seconds'), 'attempt_status': receipt.get('status'),
              'status': 'unavailable', 'tokens': None, 'estimated_usd': None,
              'price': {k: config['pricing'].get(k) for k in ('version', 'source', 'basis', 'rates')}}
    if thread is None:
        return result
    started = receipt.get('started_unix')
    if type(started) not in (int, float):
        return result
    root = Path(config.get('sessions_root', Path.home() / '.codex' / 'sessions'))
    rollout = _rollout(root, started, thread)
    module = _estimator(config)
    if rollout is not None:
        data = module.parse_rollout(rollout, thread)
        result['tokens'] = _tokens(data)
        result['status'] = data.get('status', 'unavailable')
        if result['tokens'] is None:
            result['status'] = 'unavailable'
        elif not data.get('unpriced_requests'):
            result['estimated_usd'] = data.get('estimated_usd')
        else:
            result['status'] = 'partial'
        result['models'] = [{k: row.get(k) for k in ('model', 'reasoning_effort', *TOKENS,
                                                      'estimated_usd', 'requests')}
                            for row in data.get('models', [])]
        if data.get('unpriced_requests'):
            for row in result['models']:
                if row['model'] not in config['pricing']['rates']:
                    row['estimated_usd'] = None
        return result
    # CLI completion usage is a last-turn fallback, not evidence of full session use.
    completions = [e for e in events if e.get('type') == 'turn.completed']
    tokens = _tokens(completions[-1].get('usage')) if completions else None
    if tokens is not None:
        result['tokens'] = tokens
        result['status'] = 'partial'
        result['estimated_usd'] = module.estimate(tokens, result['model'],
                                                  tokens['input_tokens'] > 272_000)
    return result


def summary(receipts):
    """Aggregate observed totals by attempt status and model; retain unknown costs."""
    groups = defaultdict(lambda: {'attempts': 0, 'elapsed_seconds': 0, 'tokens': dict.fromkeys(TOKENS, 0),
                                  'estimated_usd': 0.0, 'unknown_usage': 0, 'unpriced': 0})
    for row in receipts:
        key = (row.get('attempt_status') or 'unknown', row.get('model') or 'unknown')
        group = groups[key]
        group['attempts'] += 1
        if isinstance(row.get('elapsed_seconds'), (int, float)):
            group['elapsed_seconds'] += row['elapsed_seconds']
        tokens = row.get('tokens')
        if tokens is None:
            group['unknown_usage'] += 1
        else:
            for name in TOKENS:
                group['tokens'][name] += tokens[name]
        if row.get('estimated_usd') is None:
            group['unpriced'] += 1
        else:
            group['estimated_usd'] += row['estimated_usd']
    result = []
    for (status, model), value in sorted(groups.items()):
        value['known_estimated_usd'] = value['estimated_usd']
        if value['unpriced']:
            value['estimated_usd'] = None
        result.append({'status': status, 'model': model, **value})
    return result
