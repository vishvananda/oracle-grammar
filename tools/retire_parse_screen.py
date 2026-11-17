"""Complete-population parse/native rejection screen; never accepts a candidate."""
import json
from pathlib import Path
import subprocess

import retire as r
import retire_worker as w


def assess(run, directory, data, native_exit):
    """Require complete native evidence before classifying a semantic rejection."""
    try:
        return _assess(run, directory, data, native_exit)
    except (TypeError, AttributeError, IndexError) as error:
        raise ValueError('parse screen evidence has malformed types') from error


def _assess(run, directory, data, native_exit):
    run, directory = Path(run), Path(directory)
    if type(native_exit) is not int or native_exit not in (0, 1):
        raise ValueError('parse screen native exit is not 0 or 1')
    for name, limit in (('commands.jsonl', 1024*1024), ('native.json', 8*1024*1024),
                        ('native-failures.jsonl', 64*1024*1024), ('slice.json', 8*1024*1024),
                        ('rows.jsonl', 1024*1024*1024), ('before.jsonl', 1024*1024*1024)):
        path = directory/name
        if path.is_symlink() or not path.is_file() or path.stat().st_size > limit:
            raise ValueError('parse screen evidence missing or exceeds bound: '+name)
    commands = [json.loads(line) for line in (directory/'commands.jsonl').read_text().splitlines()]
    if any(not isinstance(item, dict) for item in commands) or [(item.get('program'), item.get('returncode')) for item in commands] != [
            ('score', 0), ('validate-readings', native_exit)]:
        raise ValueError('parse screen command receipts differ')
    index = data[0]
    count = len(index)
    manifest = r.read(run/'manifest.json')
    cards = r.read(directory/'slice.json').get('cards')
    allowed = r.read(run/'frozen/allowed.json').get('cards')
    names = {entry['name'] for entry in index.values()}
    if (type(manifest.get('population')) is not int or count != manifest['population'] or count <= 0
            or not isinstance(cards, list) or any(not isinstance(x, str) for x in cards)
            or len(cards) != len(set(cards))
            or not isinstance(allowed, list) or any(not isinstance(x, str) for x in allowed)
            or len(allowed) != len(set(allowed))
            or set(cards) != names or set(allowed) != names):
        raise ValueError('parse screen population differs')
    previous = [entry['parse'] for entry in index.values()]
    if r.rows(directory/'before.jsonl') != previous:
        raise ValueError('parse screen baseline differs')
    old_counts = [row.get('readings') for row in previous]
    if any(type(value) is not int or value < 0 for value in old_counts):
        raise ValueError('parse screen baseline reading counts are invalid')
    rows = r.rows(directory/'rows.jsonl')
    if len(rows) != count or any(not isinstance(row, dict) for row in rows):
        raise ValueError('parse screen score population differs')
    failures, new_total, recognized = [], 0, 0
    for row, entry in zip(rows, index.values(), strict=True):
        status = row.get('status')
        if status not in ('equal', 'different', 'refused'):
            raise ValueError('parse screen score status is invalid')
        readings = row.get('readings', 0)
        if type(readings) is not int or readings < 0 or (status != 'refused' and readings == 0):
            raise ValueError('parse screen reading count is invalid')
        if status == 'refused' and readings != 0:
            raise ValueError('parse screen refused face has readings')
        new_total += readings
        recognized += status != 'refused'
        if (row.get('name') != entry['name'] or row.get('text') != entry['text']
                or status != 'equal' or readings != entry['parse']['readings']):
            failures.append({'face': entry['name'], 'gate': 'parse/reading-count/text'})
    report = r.read(directory/'native.json')
    if not isinstance(report, dict) or type(report.get('compiled_schema')) is not int or report['compiled_schema'] <= 0:
        raise ValueError('parse screen native schema is invalid')
    schemas = report.get('gold_source_schemas')
    if (not isinstance(schemas, dict) or not schemas
            or any(not isinstance(k, str) or not k.isdigit() or type(v) is not int or v < 0
                   for k, v in schemas.items()) or sum(schemas.values()) != count):
        raise ValueError('parse screen source schemas do not cover the population')
    preservation, readings = report.get('reading_preservation'), report.get('readings')
    source, canonical = report.get('source_gold'), report.get('canonical_gold')
    if not all(isinstance(part, dict) for part in (preservation, readings, source, canonical)):
        raise ValueError('parse screen native report is incomplete')
    missing, additions, merged = preservation.get('missing'), preservation.get('additions'), preservation.get('merged_faces')
    if (type(preservation.get('passed')) is not bool
            or type(preservation.get('occurrences')) is not int or preservation['occurrences'] != count
            or not isinstance(missing, list) or not isinstance(additions, list)
            or type(merged) is not int or merged < 0
            or any(not isinstance(item, dict) or not isinstance(item.get('name'), str)
                   or item['name'] not in names for item in missing+additions)
            or any(type(source.get(k)) is not int or source[k] != count for k in ('faces', 'valid'))
            or any(type(canonical.get(k)) is not int or canonical[k] != count for k in ('faces', 'valid', 'idempotent'))
            or any(type(readings.get(k)) is not int or readings[k] != new_total for k in ('total', 'valid', 'canonical'))
            or type(preservation.get('old_readings')) is not int or preservation['old_readings'] != sum(old_counts)
            or any(type(report.get(k)) is not int or report[k] != recognized
                   for k in ('recognized_faces', 'faces_with_any_valid_reading', 'faces_with_all_readings_valid'))):
        raise ValueError('parse screen native report does not cover the population')
    numeric = ('explicit_readings', 'implicit_equal_singletons', 'native_normalized_distinct_readings')
    if (any(type(preservation.get(k)) is not int or preservation[k] < 0 for k in numeric)
            or preservation['explicit_readings'] + preservation['implicit_equal_singletons'] != sum(old_counts)
            or preservation['native_normalized_distinct_readings'] > sum(old_counts)
            or (merged == 0 and preservation['native_normalized_distinct_readings'] != sum(old_counts))
            or merged > count or preservation['passed'] != (not missing)
            or any(part.get('invalid_reasons') != {} for part in (source, canonical, readings))):
        raise ValueError('parse screen native counters are inconsistent')
    entries = list(index.values())
    for item in missing:
        if type(item.get('reading')) is not int or item['reading'] < 0:
            raise ValueError('parse screen missing reading is malformed')
    for item in additions:
        occurrence = item.get('occurrence')
        if (type(occurrence) is not int or not 0 <= occurrence < count
                or item['name'] != entries[occurrence]['name']
                or item.get('previous_status') != entries[occurrence]['parse'].get('status')):
            raise ValueError('parse screen addition occurrence is malformed')
        for key in ('added_current_indices', 'retained_current_indices'):
            values = item.get(key)
            if (not isinstance(values, list) or any(type(v) is not int for v in values)
                    or len(values) != len(set(values))
                    or any(not 0 <= v < rows[occurrence].get('readings', 0) for v in values)):
                raise ValueError('parse screen addition indices are malformed')
        if not item['added_current_indices']:
            raise ValueError('parse screen addition has no added reading')
    for line in (directory/'native-failures.jsonl').read_text().splitlines():
        if not isinstance(json.loads(line), dict):
            raise ValueError('parse screen native failure details are malformed')
    native_failures = w.native_failures(report, count)
    if native_exit == 1 and not native_failures:
        raise ValueError('parse screen native exit 1 lacks a preservation failure')
    if not preservation['passed'] and not (missing or additions or merged):
        raise ValueError('parse screen native rejection lacks preservation evidence')
    failures.extend(native_failures)
    return {'faces': count, 'failures': failures, 'native_exit': native_exit,
            'status': 'failed' if failures else 'passed',
            'scope': 'parse/native rejection screen; full print validation remains required'}


def run(run, directory, grammar, data, threads):
    """Run parse-only score and native validation under the caller's CPU lease."""
    run, directory = Path(run), Path(directory)
    directory.mkdir()
    names = [entry['name'] for entry in data[0].values()]
    w.subset(directory, names, data)
    r.execute(run, directory, grammar, 'score',
              [directory/'gold.jsonl', directory/'oracle.json', directory/'slice.json', directory/'rows.jsonl'], threads)
    arguments = [directory/'gold.jsonl', directory/'rows.jsonl', directory/'native.json',
                 directory/'native-failures.jsonl', '--preserve-from', directory/'before.jsonl']
    code = 0
    try:
        r.execute(run, directory, grammar, 'validate-readings', arguments, threads)
    except subprocess.CalledProcessError as error:
        command = [str((run/'frozen/bin/validate-readings').resolve()), *map(str, arguments)]
        if error.returncode != 1 or error.cmd != command:
            raise
        code = 1
    result = assess(run, directory, data, code)
    result['artifacts'] = {name: {'path': str(directory/name), 'sha256': r.digest(directory/name)}
                           for name in ('commands.jsonl', 'gold.jsonl', 'oracle.json', 'rows.jsonl', 'before.jsonl', 'slice.json',
                                        'native.json', 'native-failures.jsonl', 'score.log', 'validate-readings.log')}
    r.write(directory/'result.json', result)
    return result
