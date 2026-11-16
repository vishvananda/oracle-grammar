#!/usr/bin/env python3
"""Deterministic triage for seeded-rule retirement; inferred labels are not proofs."""
import argparse
from collections import Counter, defaultdict, OrderedDict
import hashlib
import json
from pathlib import Path
import re
import threading

import mined

MAX_RULES = 6
MAX_COHORT = 500
# These are routing hints, never an assertion that a rule has this meaning.
ROUTES = {
    'events': r'\b(?:Triggered|Event|ZoneChange|Damage|Attack|Dies|Enters)\b',
    'library': r'\b(?:Search|Shuffle|LookAt|ReorderLibrary|Library|Mill)\b',
    'continuous': r'\b(?:CreateContinuousEffect|Continuous|Change|Pump|Duration|Static)\b',
    'costs': r'\b(?:Cost|Payment|Pay|Activated|AdditionalCost)\b',
    'actions': r'\b(?:Operation|Instruction|Move|Exile|Destroy|Sacrifice|Draw|Reveal|ChooseObjects|CreateToken)\b',
    'nouns': r'\b(?:ObjectFilter|Filter|Objects|Affected|Target|Card|Creature|Graveyard|Battlefield)\b',
    'conditions': r'\b(?:Condition|Compare|If|Unless|Predicate)\b',
    'players': r'\b(?:Player|Controller|Owner|Actor|Opponent)\b',
    'mana': r'\b(?:Mana|AddMana)\b',
    'replacements': r'\b(?:Replacement|Replace|Instead)\b',
}
ATOM = re.compile(r'\b[A-Z][A-Za-z0-9_]{2,}\b')
ROOT = re.compile(r'@sem\s+([A-Za-z_]\w*)(?:::([A-Za-z_]\w*))?')
STOP = {'Ability', 'Instruction', 'Operation', 'Effect', 'Object', 'Objects', 'One',
        'Any', 'All', 'Source', 'Player', 'Players', 'AbilityController', 'Ordinary',
        'Result', 'Card', 'Program', 'Some', 'None', 'Target', 'Affected', 'Value'}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inventory(source):
    records, hashes = [], {}
    for path in mined.grammar_files(source):
        raw = path.read_bytes()
        hashes[path.relative_to(source).as_posix()] = digest(raw)
        lines = raw.decode().splitlines()
        definitions = mined.definitions(lines)
        comments = {owner: comment for index, line, owner, comment in mined.code(lines)
                    if mined.DEFINITION.match(line)}
        for name, entries in definitions.items():
            if not mined.SEEDED.fullmatch(name):
                continue
            prefix = '\n'.join(lines[i] for i in comments.get(name, []))
            sems = re.findall(r'@sem\s+(.*?)(?:\*/)', prefix, re.S)
            sem = ' '.join(sems[0].split()) if len(sems) == 1 else ''
            body = '\n'.join(line for _, line in entries).split('::=', 1)[1].strip()
            match = ROOT.search('@sem ' + sem)
            root = '::'.join(x for x in match.groups() if x) if match else 'unknown'
            literals = [json.loads(q).lower() for q in mined.QUOTED.findall(body)]
            unquoted = mined.QUOTED.sub('', body)
            nonterminals = sorted(set(mined.BARE.findall(unquoted)))
            atoms = sorted(set(ATOM.findall(sem)) - STOP)
            modules = sorted(k for k, pattern in ROUTES.items()
                             if re.search(pattern, sem, re.I))
            records.append({'name': name, 'path': path.relative_to(source).as_posix(),
                            'line': entries[0][0] + 1, 'sem': sem, 'sem_count': len(sems),
                            'root': root, 'atoms': atoms, 'literals': literals,
                            'nonterminals': nonterminals, 'modules': modules,
                            'provenance': prefix, 'body': body})
    names = [r['name'] for r in records]
    if len(set(names)) != len(names):
        raise ValueError('duplicate seeded definitions')
    return sorted(records, key=lambda r: r['name']), hashes


def anchors(rule):
    sem = rule['sem']
    operations = set(re.findall(r'\boperation:\s*([A-Z]\w*)', sem))
    event = set(re.findall(r'\bevent:\s*([A-Z]\w*)', sem))
    if rule['root'].startswith('Operation::'):
        operations.add(rule['root'].split('::', 1)[1])
    zones = set(re.findall(r'\b(?:Library|Graveyard|Battlefield|Exile|Hand|Stack)\b', sem))
    return sorted(operations), sorted(event), sorted(zones)


def signature(rule):
    # Group on actual IR responsibility, zones and structural role, not names.
    operations, events, zones = anchors(rule)
    roles = sorted(set(rule['nonterminals']) & {'cost', 'condition', 'event', 'duration'})
    # Unclassified wrappers remain independent review tasks.
    if not operations and not events:
        return (rule['root'], (), (), tuple(zones), tuple(roles), rule['body'])
    return (rule['root'], tuple(operations[:2]), tuple(events[:1]), tuple(zones), tuple(roles), '')


def coarse_signature(rule):
    """Session packing hint: root, primary action/event, and zone profile."""
    sem = rule['sem']
    events = re.findall(r'\bevent:\s*(\$?[A-Za-z_]\w*)', sem)
    operations = re.findall(r'\boperation:\s*([A-Z]\w*)', sem)
    root = rule['root']
    triggered = root.startswith('Ability::Triggered') or root.startswith('Event')
    primary = (events if triggered else operations)
    if root.startswith('Operation::'):
        primary = [root.split('::', 1)[1]]
    if not primary:
        primary = events if events else operations
    if not primary:
        # Extract the first declared change or nested operation for static and
        # wrapper forms; generic roots alone are too broad to define a family.
        shape = re.search(r'(?:changes:\s*\[|Change::\[|Operation::)([A-Z]\w*)', sem)
        if shape:
            primary = [shape.group(1)]
    if not primary and root in {'Ability::Static', 'Ability::Activated', 'Ability::Conditional',
                                'Ability::Casting', 'Ability::Scoped', 'Change', 'Operation',
                                'Instruction', 'Ability'}:
        return (root, 'unclassified-shape', (), rule['name'])
    # Missing declarations are separate review cases, not a guessed capability.
    if rule['sem_count'] != 1 or root == 'unknown':
        return (root, 'unclassified', (), rule['name'])
    zones = tuple(sorted(set(re.findall(r'\b(?:Library|Graveyard|Battlefield|Exile|Hand|Stack)\b', sem))))
    return (root, primary[0] if primary else 'no-primary', zones, '')


def build_plan(source: Path, scan: dict, *, max_rules=MAX_RULES, coarse=False) -> dict:
    if not 1 <= max_rules <= MAX_RULES:
        raise ValueError(f'max_rules must be between 1 and {MAX_RULES}')
    rules, source_hashes = inventory(Path(source))
    expected = {r['name'] for r in rules}
    needed, unused = scan['needed'], set(scan['unused'])
    if set(needed) & unused or set(needed) | unused != expected:
        raise ValueError('scan does not partition the seeded source inventory')
    groups = defaultdict(list)
    for rule in rules:
        rule['target_faces'] = sorted(set(needed.get(rule['name'], [])))
        rule['unused'] = rule['name'] in unused
        groups[(rule['unused'], coarse_signature(rule) if coarse else signature(rule))].append(rule)
    # Shared, reasonably specific referenced productions supply an additional
    # syntax neighborhood; high-fanout names are deliberately excluded.
    users = defaultdict(set)
    for rule in rules:
        for name in rule['nonterminals']:
            if not mined.SEEDED.fullmatch(name):
                users[name].add(rule['name'])
    syntax_faces = {name: sorted({face for owner in owners for face in needed.get(owner, [])})
                    for name, owners in users.items() if 2 <= len(owners) <= 100}
    jobs = []
    for key in sorted(groups, key=repr):
        group = groups[key]
        for start in range(0, len(group), max_rules):
            chunk = group[start:start + max_rules]
            names = [r['name'] for r in chunk]
            modules = sorted({m for r in chunk for m in r['modules']})
            unknown = any(r['sem_count'] != 1 or r['root'] == 'unknown' or not r['modules']
                          for r in chunk)
            kind = 'mechanical' if key[0] else 'author'
            cap = f"{chunk[0]['root']} with " + (', '.join(modules) or 'unclassified semantics')
            job_id = 'retire-' + digest('\n'.join(names).encode())[:16]
            paths = sorted({r['path'] for r in chunk} | {
                f'grammar/syntax/{m}.ebnf' for m in modules if (Path(source)/'grammar/syntax'/f'{m}.ebnf').exists()})
            jobs.append({'id': job_id, 'rules': names, 'kind': kind, 'modules': modules,
                'primarymodules': modules, 'allowedwritepaths': paths,
                'target_faces': sorted({n for r in chunk for n in r['target_faces']}),
                'capability': cap, 'semantic_atoms': sorted({a for r in chunk for a in r['atoms']}),
                'nonterminals': sorted({n for r in chunk for n in r['nonterminals']}),
                'anchors': {key: sorted({v for r in chunk for v in anchors(r)[i]})
                            for i, key in enumerate(('operations', 'events', 'zones'))},
                'syntax_neighbor_faces': sorted({face for r in chunk for n in r['nonterminals']
                                                 for face in syntax_faces.get(n, [])}),
                'source_fragments': [{k: r[k] for k in ('name', 'path', 'line', 'sem', 'body', 'provenance')}
                                     for r in chunk],
                'goal': ('Diagnose unknown semantics and dependencies; ' if unknown else '') +
                        ('Review and remove unused seeded productions.' if kind == 'mechanical' else
                         'Implement reusable compositional coverage and retire assigned seeded productions.'),
                'rule_dispositions_required': names,
                'dependency_hints': ['Report a separate diagnostic disposition for every assigned rule.',
                    'Inspect shared nonterminal users, operands, scope and identity.',
                    'Cohorts are triage only; a full batch audit is required.'],
                'requires_diagnosis': unknown, 'labels_are_inferred': True})
    jobs.sort(key=lambda j: j['id'])
    assigned = [name for job in jobs for name in job['rules']]
    counts = Counter(assigned)
    return {'jobs': jobs, 'rules': rules,
            'counts': {'expected': len(expected), 'assigned': len(assigned), 'jobs': len(jobs),
                       'unused': len(unused), 'needed': len(needed)},
            'manifest': {'expected_rules': sorted(expected), 'assigned_rules': sorted(assigned),
                         'missing': sorted(expected - counts.keys()),
                         'duplicates': sorted(n for n, c in counts.items() if c != 1)},
            'inputs': {'source_grammar_sha256': source_hashes,
                       'scan_canonical_sha256': digest(json.dumps(scan, sort_keys=True, separators=(',', ':')).encode())},
            'packing': 'coarse' if coarse else 'fine',
            'notice': 'Mechanically inferred triage labels are not semantic proof or acceptance.'}


class CohortReview(ValueError):
    """Required cohort exceeds the limit; no partial cohort is returned."""
    def __init__(self, count, bound):
        self.count, self.bound = count, bound
        super().__init__(f'coordinator review: {count} candidate faces exceed bound {bound}; no truncation')


def prepare_cohort_index(index, gold):
    """Index structural operations/events and zones once over allowed gold faces."""
    allowed = {e['name'] for e in index.values()}
    features = {k: defaultdict(set) for k in ('operations', 'events', 'zones')}
    zones = {'Library', 'Graveyard', 'Battlefield', 'Exile', 'Hand', 'Stack'}

    def walk(node, found):
        if isinstance(node, list):
            for item in node:
                walk(item, found)
        elif isinstance(node, dict):
            operation = node.get('operation')
            if isinstance(operation, dict) and isinstance(operation.get('kind'), str):
                found['operations'].add(operation['kind'])
            event = node.get('event')
            if isinstance(event, dict):
                if isinstance(event.get('kind'), str):
                    found['events'].add(event['kind'])
                found['events'].update(k for k in event if k[:1].isupper())
            for key, value in node.items():
                if key in zones:
                    found['zones'].add(key)
                if isinstance(value, str) and value in zones:
                    found['zones'].add(value)
                walk(value, found)
    seen = set()
    for record in gold:
        for face in record['faces']:
            name = face['name']
            if name not in allowed:
                raise ValueError('gold face outside allowed population')
            seen.add(name)
            found = {key: set() for key in features}
            walk(face.get('abilities', []), found)
            for key, values in found.items():
                for value in values:
                    features[key][value].add(name)
    if seen != allowed:
        raise ValueError('gold and allowed index populations differ')
    return {'allowed': allowed, 'features': features, 'neighborhood_cache': OrderedDict(),
            'neighborhood_lock': threading.Lock()}


def select_cohort_detail(job, index, gold, scan, bound=MAX_COHORT):
    """Return targets plus a bounded structural neighborhood, with scope metadata.

    This heuristic is diagnostic and cannot certify dependencies; full batch audit
    and companion-face expansion are mandatory coordinator responsibilities.
    """
    if not 1 <= bound <= MAX_COHORT:
        raise ValueError(f'bound must be between 1 and {MAX_COHORT}')
    prepared = index if 'features' in index and 'allowed' in index else prepare_cohort_index(index, gold)
    target = set(job.get('target_faces', []))
    for rule in job['rules']:
        target.update(scan['needed'].get(rule, []))
    if not target <= prepared['allowed']:
        raise ValueError('target faces outside allowed population')
    if len(target) > bound:
        raise CohortReview(len(target), bound)
    anchor = job.get('anchors', {})
    features = prepared['features']
    key = tuple(tuple(anchor.get(k, [])) for k in ('operations', 'events', 'zones'))
    cache = prepared.setdefault('neighborhood_cache', OrderedDict())
    lock = prepared.setdefault('neighborhood_lock', threading.Lock())
    with lock:
        if key not in cache:
            families = []
            # Intersect available operation and event families, then narrow by zone.
            for kind in ('operations', 'events'):
                matches = set()
                for value in anchor.get(kind, []):
                    matches.update(features[kind].get(value, ()))
                if anchor.get(kind):
                    families.append(matches)
            candidates = set.intersection(*families) if families else set()
            zone_matches = set()
            for value in anchor.get('zones', []):
                zone_matches.update(features['zones'].get(value, ()))
            if anchor.get('zones'):
                candidates &= zone_matches
            matches = Counter()
            for kind in ('operations', 'events', 'zones'):
                for value in anchor.get(kind, []):
                    for name in features[kind].get(value, set()) & candidates:
                        matches[name] += 1
            cache[key] = sorted(candidates, key=lambda n: (-matches[n], n))
            if len(cache) > 128:
                cache.popitem(last=False)
        else:
            cache.move_to_end(key)
        structural = [name for name in cache[key] if name not in target]
    syntax = set(job.get('syntax_neighbor_faces', []))
    if not syntax <= prepared['allowed']:
        raise ValueError('syntax neighbor faces outside allowed population')
    # Prefer structural matches, then include shared-production users.
    ordered = structural + sorted(syntax - target - set(structural))
    capacity = bound - len(target)
    sample = ordered[:capacity]
    sampled = len(ordered) > capacity
    return {'names': sorted(target | set(sample)), 'target_count': len(target),
            'neighborhood_count': len(ordered), 'sampled': sampled,
            'scope': 'gold IR operation/event and zone co-occurrence plus seeded shared-production users; heuristic, not dependency proof',
            'reason': ('bounded diagnostic sample of inferred neighbors' if sampled else
                       'all inferred neighbors' if ordered else
                       'no neighbors inferred; targets only'),
            'full_batch_audit_required': True}


def select_cohort(job, index, gold, scan, bound=MAX_COHORT):
    """Compatibility helper returning names; use select_cohort_detail for scope metadata."""
    detail = select_cohort_detail(job, index, gold, scan, bound)
    if detail['sampled']:
        raise CohortReview(detail['target_count'] + detail['neighborhood_count'], bound)
    return detail['names']


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('scan', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--max-rules', type=int, default=MAX_RULES)
    parser.add_argument('--coarse', action='store_true', help='pack related primary capabilities into sessions')
    args = parser.parse_args()
    # Output is diagnostic data and must stay in the ignored cache or target tree.
    dest = args.output.resolve()
    if not any(part in ('.cache', 'target') for part in dest.parts):
        parser.error('output must be under .cache/ or target/')
    plan = build_plan(args.source, json.loads(args.scan.read_text()), max_rules=args.max_rules, coarse=args.coarse)
    plan['inputs']['scan_file_sha256'] = digest(args.scan.read_bytes())
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(plan, indent=2, sort_keys=True) + '\n')


if __name__ == '__main__':
    main()
