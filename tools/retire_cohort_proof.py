"""Evidence for retrying an old one-pass shared-name cohort defect."""
from collections import deque
import hashlib
import json
from pathlib import Path

import retire

MAX_BYTES = 128 * 1024 * 1024
MAX_OCCURRENCES = 1024


def _read(path, expected_hash=None):
    if path.is_symlink():
        raise ValueError('proof file is a symlink')
    with path.open('rb') as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError('cohort proof input exceeds byte limit')
    if expected_hash is not None and hashlib.sha256(data).hexdigest() != expected_hash:
        raise ValueError('cohort proof hash differs')
    return data


def _artifact(evidence, key, expected):
    spec = evidence.get(key)
    if not isinstance(spec, dict) or not isinstance(spec.get('path'), str) or not isinstance(spec.get('sha256'), str):
        raise ValueError('cohort proof needs a hashed '+key)
    path = Path(spec['path'])
    if not path.is_absolute() or path.is_symlink() or path.resolve() != expected.resolve():
        raise ValueError('cohort proof path differs: '+key)
    return _read(path, spec['sha256'])


def _jsonl(data):
    return [json.loads(line) for line in data.splitlines() if line.strip()]


def _load(campaign, job, attempt, base, original, evidence, rules):
    if not isinstance(evidence, dict) or evidence.get('kind') != 'shared-name-cohort-closure':
        raise ValueError('shared-name cohort proof is required')
    terminal = evidence.get('controller_terminal')
    if (not isinstance(terminal, dict)
            or not isinstance(terminal.get('unit'), str) or not terminal['unit']
            or not isinstance(terminal.get('invocation'), str) or not terminal['invocation']
            or not isinstance(terminal.get('verification'), dict) or not terminal['verification']):
        raise ValueError('terminal controller proof is required')
    run = campaign/'.cache/attempts'/attempt
    report_path = Path(original.get('report', ''))
    evaluations = run/'evaluations'/job
    if (not report_path.is_absolute() or report_path.name != 'report.json'
            or not report_path.resolve().is_relative_to(evaluations.resolve())):
        raise ValueError('report is outside the assigned evaluation')
    report = json.loads(_artifact(evidence, 'report', report_path))
    directory = report_path.parent
    if (report.get('task') != job or report.get('source') != base or report.get('passed') is not False
            or Path(report.get('directory', '')).resolve() != directory.resolve()
            or report.get('family_returncode') != 0):
        raise ValueError('report is not the failed assigned evaluation')
    failures = report.get('failures')
    if not isinstance(failures, list) or len(failures) != 1 or 'validate-readings' not in str(failures[0]):
        raise ValueError('evaluation has another or unproved failure')
    descriptor_data = _artifact(evidence, 'descriptor', run/'tasks'/f'{job}.json')
    if hashlib.sha256(descriptor_data).hexdigest() != report.get('descriptor_sha256'):
        raise ValueError('descriptor differs from independent report')
    descriptor = json.loads(descriptor_data)
    if descriptor.get('id') != job or descriptor.get('rules') != rules:
        raise ValueError('descriptor does not match assigned rules')
    support_hash = report.get('support_sha256')
    if not isinstance(support_hash, str) or len(support_hash) != 64:
        raise ValueError('support hash is missing from report')
    support_data = _read(run/'support.json', support_hash)
    if json.loads(support_data).get('tasks', {}).get(job) != report.get('descriptor_sha256'):
        raise ValueError('support does not bind descriptor')
    log = _artifact(evidence, 'log', directory/'validate-readings.log')
    if log.strip() != b'Error: "old/current score order or populations differ"':
        raise ValueError('diagnostic failure is not the old cohort mismatch')
    commands = _jsonl(_artifact(evidence, 'commands', directory/'commands.jsonl'))
    if [(x.get('program'), x.get('returncode')) for x in commands] != [
            ('family-check', 0), ('score', 0), ('validate-readings', 1)]:
        raise ValueError('cohort check commands do not prove the isolated failure')
    checkpoint = json.loads(_read(campaign/'checkpoints'/f'{base}.json'))
    inputs = run/'inputs'
    if inputs.resolve() != Path(checkpoint['inputs']).resolve():
        raise ValueError('frozen inputs do not match assigned checkpoint')
    manifest = json.loads(_artifact(evidence, 'manifest', inputs/'manifest.json'))
    if manifest.get('commit') != base or manifest.get('format') != 3:
        raise ValueError('frozen manifest does not match attempt base')
    files = manifest['files']
    index = json.loads(_read(inputs/'frozen/index.json', files['index.json']))
    gold = _jsonl(_read(inputs/'frozen/gold.jsonl', files['gold.jsonl']))
    actual = {key: _jsonl(_artifact(evidence, key, directory/(key+'.jsonl')))
              for key in ('gold', 'before', 'rows')}
    return descriptor, index, gold, actual


def _prove(descriptor, index, gold, actual):
    names = descriptor.get('cohort')
    if (not isinstance(names, list) or not names or len(names) > 500
            or any(not isinstance(name, str) or not name for name in names) or len(set(names)) != len(names)):
        raise ValueError('invalid original bounded cohort')
    if not isinstance(index, dict) or list(index) != [str(n) for n in range(len(index))]:
        raise ValueError('frozen index order differs')
    initial = set(names)
    entries = list(index.values())
    initial_records = [record for record in gold if any(face['name'] in initial for face in record['faces'])]
    if initial_records != actual['gold']:
        raise ValueError('recorded gold is not the old one-pass selection')
    old_names = {face['name'] for record in initial_records for face in record['faces']}
    expected_before = [entry['parse'] for entry in entries if entry['name'] in old_names]
    if expected_before != actual['before']:
        raise ValueError('recorded before rows do not match old one-pass selection')
    # Preserve record addresses even when two gold records have identical contents.
    old_addresses = [(i, j) for i, record in enumerate(gold)
                     if any(face['name'] in initial for face in record['faces'])
                     for j in range(len(record['faces']))]
    addresses = {}
    for entry in entries:
        address = (entry['record'], entry['face'])
        if address in addresses:
            raise ValueError('frozen index repeats a face address')
        if (type(address[0]) is not int or type(address[1]) is not int
                or address[0] < 0 or address[0] >= len(gold)
                or address[1] < 0 or address[1] >= len(gold[address[0]]['faces'])
                or gold[address[0]]['faces'][address[1]]['name'] != entry['name']):
            raise ValueError('frozen index face address differs')
        addresses[address] = entry
    if len(actual['rows']) != len(old_addresses) or len(expected_before) == len(old_addresses):
        raise ValueError('old cohort does not exhibit an occurrence mismatch')
    for row, address in zip(actual['rows'], old_addresses, strict=True):
        old = addresses[address]
        if (row.get('status') != 'equal' or row.get('name') != old['name']
                or row.get('text') != old['text'] or row.get('readings') != old['parse']['readings']
                or any(before and not after for before, after in zip(old['flags'], retire.flags(row), strict=True))):
            raise ValueError('score rows contain a separate semantic or print failure')
    by_name = {}
    for address, entry in addresses.items():
        by_name.setdefault(entry['name'], []).append(address)
    if not initial <= by_name.keys():
        raise ValueError('cohort contains a name outside the frozen index')
    selected, pending, records, observed = set(initial), deque(sorted(initial)), set(), set()
    while pending:
        for address in by_name[pending.popleft()]:
            observed.add(address)
            record = address[0]
            if record in records:
                continue
            records.add(record)
            for face in gold[record]['faces']:
                name = face['name']
                if name not in by_name:
                    raise ValueError('closure contains an unindexed companion')
                if name not in selected:
                    selected.add(name)
                    pending.append(name)
        if sum(len(gold[i]['faces']) for i in records) > MAX_OCCURRENCES:
            raise ValueError('complete closure exceeds diagnostic occurrence bound')
    expected = {(i, j) for i in records for j in range(len(gold[i]['faces']))}
    if observed != expected or len(expected) <= len(old_addresses):
        raise ValueError('complete closure is not a strict, fully indexed expansion')
    return {'original_records': len(initial_records), 'original_occurrences': len(old_addresses),
            'old_before_occurrences': len(expected_before), 'closed_records': len(records),
            'closed_occurrences': len(expected), 'closed_names': len(selected)}


def verify(campaign, job, attempt, base, original, evidence, rules):
    try:
        return _prove(*_load(campaign, job, attempt, base, original, evidence, rules))
    except (OSError, ValueError, KeyError, TypeError, IndexError, UnicodeError) as exc:
        raise ValueError('invalid cohort retry proof: '+str(exc)) from exc
