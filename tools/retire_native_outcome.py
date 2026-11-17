"""Classify a complete native preservation rejection without changing T5 tools."""
import json
from pathlib import Path
import subprocess

import retire as r
import retire_worker as w


def preservation_failure(error, run, directory, data):
    """Recover only a complete exit-1 preservation failure; never infer a pass."""
    run, directory = Path(run), Path(directory)
    command = [str((run/'frozen/bin/validate-readings').resolve()), str(directory/'gold.jsonl'),
               str(directory/'rows.jsonl'), str(directory/'native.json'), str(directory/'native-failures.jsonl'),
               '--preserve-from', str(directory/'before.jsonl')]
    if not isinstance(error, subprocess.CalledProcessError) or error.returncode != 1 or error.cmd != command:
        raise error
    for name, limit in (('commands.jsonl', 1024*1024), ('native.json', 8*1024*1024),
                        ('native-failures.jsonl', 64*1024*1024), ('slice.json', 8*1024*1024),
                        ('rows.jsonl', 1024*1024*1024), ('before.jsonl', 1024*1024*1024)):
        path = directory/name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
            raise ValueError('native preservation evidence is unavailable or exceeds bounds')
    commands = [json.loads(line) for line in (directory/'commands.jsonl').read_text().splitlines()]
    if (len(commands) != 2 or [(c.get('program'), c.get('returncode')) for c in commands]
            != [('score', 0), ('validate-readings', 1)]):
        raise ValueError('native preservation command receipts differ')
    cards = r.read(directory/'slice.json').get('cards')
    index = data[0]
    expected_names = {entry['name'] for entry in index.values()}
    if (not isinstance(cards, list) or any(not isinstance(name, str) for name in cards)
            or len(cards) != len(set(cards)) or set(cards) != expected_names):
        raise ValueError('native preservation population differs')
    count = len(index)
    if not count or r.rows(directory/'before.jsonl') != [entry['parse'] for entry in index.values()]:
        raise ValueError('native preservation baseline is incomplete or changed')
    old_counts = [entry['parse'].get('readings') for entry in index.values()]
    if any(type(value) is not int or value < 0 for value in old_counts):
        raise ValueError('native preservation baseline reading counts are invalid')
    report = r.read(directory/'native.json')
    if not isinstance(report, dict):
        raise ValueError('native preservation report is invalid')
    preservation, readings = report.get('reading_preservation'), report.get('readings')
    source, canonical = report.get('source_gold'), report.get('canonical_gold')
    if not all(isinstance(part, dict) for part in (preservation, readings, source, canonical)):
        raise ValueError('native preservation report is incomplete')
    missing, additions = preservation.get('missing'), preservation.get('additions')
    merged = preservation.get('merged_faces')
    if (type(preservation.get('passed')) is not bool or preservation['passed']
            or type(preservation.get('occurrences')) is not int or preservation['occurrences'] != count
            or not isinstance(missing, list) or not isinstance(additions, list)
            or type(merged) is not int or merged < 0 or not (missing or additions or merged)
            or any(not isinstance(item, dict) or item.get('name') not in expected_names for item in missing+additions)
            or any(type(source.get(k)) is not int or source[k] != count for k in ('faces', 'valid'))
            or any(type(canonical.get(k)) is not int or canonical[k] != count for k in ('faces', 'valid', 'idempotent'))
            or any(type(readings.get(k)) is not int or readings[k] < 0 for k in ('total', 'valid', 'canonical'))
            or readings['total'] != readings['valid'] or readings['total'] != readings['canonical']
            or type(preservation.get('old_readings')) is not int
            or preservation['old_readings'] != sum(old_counts)):
        raise ValueError('native report does not prove a complete preservation rejection')
    # The CLI's detail stream may be empty when it records preservation changes in native.json.
    for line in (directory/'native-failures.jsonl').read_text().splitlines():
        if not isinstance(json.loads(line), dict):
            raise ValueError('native failure details are malformed')
    failures = w.native_failures(report, count)
    failures.extend(w.compare(r.rows(directory/'rows.jsonl'), index, set(cards)))
    if not failures:
        raise ValueError('native exit 1 did not produce a semantic failure')
    return {'faces': count, 'failures': failures}
