#!/usr/bin/env python3
"""Score hash-verified corpus cohorts and compare them with a baseline by occurrence.

Inputs are named only in the ignored local manifest `.cache/corpus/inputs.json`:

    {"allowed":   {"path": ..., "sha256": ...},   explicit unsealed population
     "oracle":    {"path": ..., "sha256": ...},   prepared Oracle records
     "reference": {"path": ..., "sha256": ...},   canonical native reference JSONL
     "baseline":  {"path": ..., "sha256": ...}}   score rows of the whole population

Nothing here bundles corpus data. Outputs stay under `.cache/corpus/`.
"""
import argparse
from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import re
import sys
import tempfile

import dev
import pin

WORK = dev.ROOT/'.cache/corpus'
MANIFEST = WORK/'inputs.json'
ISSUES = dev.ROOT/'adapters/mtgish/reference-issues.md'
INPUTS = ('allowed', 'oracle', 'reference', 'baseline')
TRANSFORM = 1  # Bump when the allowed-population filter changes.


def rows(path):
    with Path(path).open() as stream:
        return [json.loads(line) for line in stream]


def write_rows(path, values):
    with Path(path).open('w') as stream:
        for value in values:
            stream.write(json.dumps(value, ensure_ascii=False)+'\n')


def write(path, value):
    Path(path).write_text(json.dumps(value, indent=2, ensure_ascii=False)+'\n')


def resolve(spec):
    path = Path(spec['path'])
    return path if path.is_absolute() else dev.ROOT/path


def inputs():
    data = json.loads(MANIFEST.read_text())
    for name in INPUTS:
        if pin.digest(resolve(data[name])) != data[name]['sha256']:
            raise ValueError(f'{name} checksum mismatch: {resolve(data[name])}')
    return data


def allowed(data):
    return set(json.loads(resolve(data['allowed']).read_text())['cards'])


def prepare(data):
    """Filter Oracle and reference to the allowed faces once; verify derived bytes on reuse."""
    identity = {name: data[name]['sha256'] for name in ('allowed', 'oracle', 'reference')}
    identity['transform'] = TRANSFORM
    key = pin.hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    destination = WORK/'prepared'/key
    if destination.exists():
        receipt = json.loads((destination/'manifest.json').read_text())
        if receipt['inputs'] != identity or any(
                pin.digest(destination/name) != value for name, value in receipt['files'].items()):
            raise ValueError(f'prepared inputs changed: {destination}')
        return destination
    wanted = allowed(data)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as directory:
        temporary = Path(directory)
        # Keep record order and parent identity; drop only faces outside the population.
        records = []
        for record in json.loads(resolve(data['oracle']).read_text()):
            cards = [face for face in record['cards'] if face.get('name') in wanted]
            if cards:
                records.append({'cards': cards})
        write(temporary/'oracle.json', records)
        with resolve(data['reference']).open() as source, (temporary/'gold.jsonl').open('w') as out:
            for line in source:
                record = json.loads(line)
                faces = [face for face in record.get('faces', []) if face.get('name') in wanted]
                if faces:
                    record['faces'] = faces
                    out.write(json.dumps(record)+'\n')
        files = {name: pin.digest(temporary/name) for name in ('oracle.json', 'gold.jsonl')}
        write(temporary/'manifest.json', {'inputs': identity, 'files': files})
        temporary.rename(destination)
    return destination


def summary(values):
    statuses = Counter(row['status'] for row in values)
    return {'faces': len(values), 'statuses': dict(sorted(statuses.items())),
            'recognized': len(values)-statuses['refused'],
            'one_reading': sum(row.get('readings') == 1 for row in values),
            'readings': sum(row.get('readings') or 0 for row in values)}


def documented():
    """Faces with a recorded reference issue: each `### Name` heading of the issue log."""
    if not ISSUES.exists():
        return frozenset()
    return frozenset(line[4:].strip() for line in ISSUES.read_text().splitlines()
                     if line.startswith('### '))


def compare(before, after, issues=frozenset()):
    """Classify changed occurrences. A lost parse, a limit refusal or a lost match fails,
    unless the lost match is a still-recognized face with a recorded reference issue."""
    if [r['name'] for r in before] != [r['name'] for r in after]:
        raise ValueError('baseline and current face order or population differ')
    if any(a.get('text') != b.get('text') for a, b in zip(before, after)):
        raise ValueError('baseline and current input text differ')
    changed = [{'occurrence': i, 'name': a['name'], 'before': a['status'], 'after': b['status'],
                'before_readings': a.get('readings', 0), 'after_readings': b.get('readings', 0)}
               for i, (a, b) in enumerate(zip(before, after)) if a != b]
    def names(test):
        return [c['name'] for c in changed if test(c)]
    report = {'before': summary(before), 'after': summary(after),
              'gained_matches': names(lambda c: c['before'] != 'equal' and c['after'] == 'equal'),
              'lost_matches': names(lambda c: c['before'] == 'equal' and c['after'] != 'equal'),
              'lost_parses': names(lambda c: c['before'] != 'refused' and c['after'] == 'refused'),
              'new_parses': names(lambda c: c['before'] == 'refused' and c['after'] != 'refused'),
              'reading_counts': names(lambda c: c['before_readings'] != c['after_readings']),
              'limits': [r['name'] for r in after if str(r.get('why', '')).startswith('Limit')],
              'changed': changed}
    report['documented_losses'] = [name for name in report['lost_matches']
                                   if name in issues and name not in report['lost_parses']]
    unexplained = [name for name in report['lost_matches'] if name not in report['documented_losses']]
    report['passed'] = not (unexplained or report['lost_parses'] or report['limits'])
    return report


def cohort(baseline, cards):
    return [row for row in baseline if row['name'] in cards]


def slice_path(name):
    if not re.fullmatch(r'[a-z0-9][a-z0-9-]*', name):
        raise ValueError('slice names use lowercase letters, digits and hyphens')
    return WORK/'slices'/f'{name}.json'


def make_slice(args):
    data = inputs()
    population = allowed(data)
    baseline = rows(resolve(data['baseline']))
    cards = set(args.cards or ())
    if args.all:
        cards |= population
    if args.unmatched:
        cards |= {row['name'] for row in baseline if row['status'] != 'equal'}
    if args.regex:
        pattern = re.compile(args.regex, re.I)
        oracle = json.loads((prepare(data)/'oracle.json').read_text())
        cards |= {face['name'] for record in oracle for face in record['cards']
                  if pattern.search(face.get('text', ''))}
    if not cards or not cards <= population or not cards <= {row['name'] for row in baseline}:
        raise ValueError('empty slice or a face outside the allowed population or baseline')
    path = slice_path(args.name)
    path.parent.mkdir(parents=True, exist_ok=True)
    write(path, {'cards': sorted(cards)})
    print(f'{args.name}: {len(cards)} faces')


def score(args):
    data = inputs()
    prepared = prepare(data)
    cards = set(json.loads(slice_path(args.name).read_text())['cards'])
    out = WORK/'runs'/args.name
    out.mkdir(parents=True, exist_ok=True)
    flags = [f for f, on in (('--render', args.render), ('--round-trip', args.round_trip)) if on]
    with dev.lock(shared=True), (out/'score.log').open('w') as log:
        dev.run('score', [str(prepared/'gold.jsonl'), str(prepared/'oracle.json'),
                          str(slice_path(args.name)), str(out/'rows.jsonl'),
                          f'--totals={out/"totals.json"}', *flags], stdout=log)
    before = cohort(rows(resolve(data['baseline'])), cards)
    write_rows(out/'before.jsonl', before)
    report = compare(before, rows(out/'rows.jsonl'), documented())
    report.update(slice=args.name, baseline_sha256=data['baseline']['sha256'],
                  rows_sha256=pin.digest(out/'rows.jsonl'))
    write(out/'changes.json', report)
    print(json.dumps({'before': report['before'], 'after': report['after']}))
    for field in ('gained_matches', 'lost_matches', 'documented_losses', 'lost_parses',
                  'new_parses', 'reading_counts', 'limits'):
        if report[field]:
            print(f'{field} ({len(report[field])}): ' + '; '.join(report[field][:12]))
    if not report['passed']:
        raise ValueError(f'{args.name} lost matches, parses or hit limits; see {out/"changes.json"}')


def audit(args):
    """Native validity of every saved reading and preservation of every baseline meaning."""
    data = inputs()
    out = WORK/'runs'/args.name
    with dev.lock(shared=True), (out/'audit.log').open('w') as log:
        dev.run('validate-readings', [str(prepare(data)/'gold.jsonl'), str(out/'rows.jsonl'),
                str(out/'audit.json'), str(out/'audit-failures.jsonl'),
                '--preserve-from', str(out/'before.jsonl')], stdout=log)
    report = json.loads((out/'audit.json').read_text())
    preservation = report['reading_preservation']
    readings = report['readings']
    print(json.dumps({'readings': readings, 'preserved': preservation['passed'],
                      'missing': len(preservation['missing']),
                      'added_faces': [a['name'] for a in preservation['additions']]}))
    if not preservation['passed'] or readings['valid'] != readings['total']:
        raise ValueError(f'native audit failed; see {out/"audit.json"}')


def rules(args):
    """Which seeded rules a slice needs; every other seeded rule is a deletion candidate."""
    data = inputs()
    prepared = prepare(data)
    out = WORK/'runs'/args.name
    out.mkdir(parents=True, exist_ok=True)
    flags = ['--credit'] if args.credit else []
    with dev.lock(shared=True), (out/'rules.log').open('w') as log:
        dev.run('rule-audit', [str(prepared/'gold.jsonl'), str(prepared/'oracle.json'),
                               str(slice_path(args.name)), str(out/'rules.json'), *flags],
                stdout=log)
    report = json.loads((out/'rules.json').read_text())
    print(json.dumps(report['summary']))
    for face in report['unresolved'][:12]:
        print(f"unresolved: {face['name']}: {face['why']}")


def differences(gold, ours, path=()):
    """Minimal differing nodes: a kind change stops descent; lists compare by position."""
    if isinstance(gold, dict) and isinstance(ours, dict) and gold.get('kind') == ours.get('kind'):
        for key in sorted(gold.keys() | ours.keys()):
            if gold.get(key) != ours.get(key):
                yield from differences(gold.get(key), ours.get(key), path+(key,))
    elif isinstance(gold, list) and isinstance(ours, list) and len(gold) == len(ours):
        for index, (a, b) in enumerate(zip(gold, ours)):
            if a != b:
                yield from differences(a, b, path+(index,))
    elif gold != ours:
        yield path, gold, ours


def label(value):
    if isinstance(value, dict) and isinstance(value.get('kind'), str):
        return value['kind']
    if isinstance(value, list):
        return f'list[{len(value)}]'
    return json.dumps(value)[:40]


def signature(row):
    """Triage key only: the shared mechanism still needs review against Oracle meaning."""
    if row['status'] == 'refused':
        why = str(row.get('why', ''))
        frontier = row.get('syntax', {}).get('frontier')
        if frontier and '^' in frontier:
            after = frontier.split('^', 1)[1].split()
            return 'syntax before: ' + (' '.join(after[:2]).lower() or '<end>'), frontier
        return why.split('(')[0] + ': ' + why[len(why.split('(')[0])+1:][:60], why
    readings = row.get('all') or [row['ours']]
    found = min((list(differences(row['gold'], reading)) for reading in readings),
                key=lambda nodes: sum(len(json.dumps(g))+len(json.dumps(o)) for _, g, o in nodes))
    path, gold, ours = found[0]
    where = '/'.join(str(p) for p in path if isinstance(p, str))[-48:]
    return f'{label(gold)} -> {label(ours)} at {where}', [
        {'path': list(p), 'gold': g, 'ours': o} for p, g, o in found[:3]]


def queue(args):
    data = inputs()
    source = WORK/'runs'/args.name/'rows.jsonl' if args.name else resolve(data['baseline'])
    clusters, details = defaultdict(list), []
    for row in rows(source):
        if row['status'] != 'equal':
            key, evidence = signature(row)
            clusters[key].append(row['name'])
            details.append({'name': row['name'], 'status': row['status'], 'key': key,
                            'text': row['text'], 'evidence': evidence})
    write(WORK/'queue.json', details)
    for key, names in sorted(clusters.items(), key=lambda item: (-len(item[1]), item[0])):
        print(f'{len(names):4}  {key}  | ' + '; '.join(names[:4]))


def overlay(baseline, current, cards):
    """Replace by occurrence, never by name: duplicate faces stay independent."""
    replacements = iter(current)
    result = [next(replacements) if row['name'] in cards else row for row in baseline]
    if next(replacements, None) is not None:
        raise ValueError('current rows outnumber the baseline cohort')
    return result


def accept(args):
    data = inputs()
    out = WORK/'runs'/args.name
    report = json.loads((out/'changes.json').read_text())
    if (not report['passed'] or report['baseline_sha256'] != data['baseline']['sha256']
            or report['rows_sha256'] != pin.digest(out/'rows.jsonl')):
        raise ValueError('only an unchanged, passing run against the current baseline is accepted')
    cards = set(json.loads(slice_path(args.name).read_text())['cards'])
    result = overlay(rows(resolve(data['baseline'])), rows(out/'rows.jsonl'), cards)
    pending = WORK/'baselines'/'pending.jsonl'
    pending.parent.mkdir(parents=True, exist_ok=True)
    write_rows(pending, result)
    target = pending.with_name(pin.digest(pending)+'.jsonl')
    pending.replace(target)
    data['baseline'] = {'path': str(target.relative_to(dev.ROOT)), 'sha256': pin.digest(target)}
    write(MANIFEST.with_suffix('.next'), data)
    os.replace(MANIFEST.with_suffix('.next'), MANIFEST)
    print(json.dumps({'baseline': data['baseline'], 'summary': summary(result)}))


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    commands = parser.add_subparsers(dest='command', required=True)
    selection = commands.add_parser('slice', help='write a named cohort of face names')
    selection.add_argument('name')
    selection.add_argument('--all', action='store_true')
    selection.add_argument('--unmatched', action='store_true')
    selection.add_argument('--cards', nargs='+')
    selection.add_argument('--regex', help='case-insensitive search of prepared Oracle text')
    scoring = commands.add_parser('score', help='score a slice and compare with the baseline')
    scoring.add_argument('name')
    scoring.add_argument('--render', action='store_true')
    scoring.add_argument('--round-trip', action='store_true')
    for command, help_text in (('audit', 'native validity and baseline-meaning preservation'),
                               ('accept', 'overlay a passing run onto the baseline')):
        commands.add_parser(command, help=help_text).add_argument('name')
    needs = commands.add_parser('rules', help='seeded rules a slice needs, and the unused rest')
    needs.add_argument('name')
    needs.add_argument('--credit', action='store_true', help='also tally right and wrong faces')
    ranking = commands.add_parser('queue', help='cluster unmatched rows by shared mechanism')
    ranking.add_argument('name', nargs='?')
    commands.add_parser('prepare', help='verify inputs and filter them to the population')
    args = parser.parse_args()
    if args.command == 'prepare':
        print(prepare(inputs()))
    else:
        {'slice': make_slice, 'score': score, 'audit': audit, 'rules': rules,
         'queue': queue, 'accept': accept}[args.command](args)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, KeyError, dev.subprocess.CalledProcessError) as error:
        sys.exit(str(error))
