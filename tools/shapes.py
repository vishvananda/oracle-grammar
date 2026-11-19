#!/usr/bin/env python3
"""Grammar shapes that make print search blow up (docs/print-shapes.md).

An agreement alternation offers two inflections of one word in one production,
as in `("gets" | "get")`. Number is decided by the subject or a count elsewhere,
so the printer builds both spellings, and the wrong one survives until style
scoring or a full semantic bind rejects it. Each such group doubles a card's
candidates. A gendered pronoun alternation, `"it" | "he" | "she"`, offers every
pronoun for one referent whose gender the production cannot see.

`python3 tools/shapes.py` lists every occurrence. The baseline lists the ones
that remain to be split by number or context. New occurrences fail, and fixed
ones must leave the baseline, so the count only falls.
"""
from pathlib import Path
import json
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
BASELINE_PATH = 'tools/shape_baseline.json'
BASELINE = ROOT/BASELINE_PATH
IRREGULAR = {('has', 'have'), ('is', 'are'), ('was', 'were'), ('does', 'do')}
NOT_AGREEMENT = {('it', 'its')}
GENDERED = {'he', 'she', 'him', 'her', 'his', 'hers', 'himself', 'herself'}
RULE = re.compile(r'^(\w+) ::= (.*)$')


def alternations(body):
    """Each alternation's members that are single quoted words, top level or grouped."""
    groups = re.findall(r'\(([^()]*)\)', body)
    groups.append(re.sub(r'\([^()]*\)', '()', body))
    for group in groups:
        members = [m.strip() for m in group.split('|')]
        words = [m[1:-1] for m in members if re.fullmatch(r'"[a-z\']+"', m)]
        if len(members) > 1 and len(words) > 1:
            yield words


def inflections(words):
    pairs = set()
    for a in words:
        for b in words:
            if (b in (a + 's', a + 'es') or (a, b) in IRREGULAR) and (a, b) not in NOT_AGREEMENT:
                pairs.add(f'{a}|{b}')
    return pairs


def found(root=ROOT):
    """{rule: sorted shape labels} for every grammar production."""
    shapes = {}
    for path in sorted((root/'grammar').rglob('*.ebnf')):
        for line in path.read_text().splitlines():
            m = RULE.match(line)
            if not m:
                continue
            labels = set()
            for words in alternations(m.group(2)):
                labels |= {f'agreement {pair}' for pair in inflections(words)}
                gendered = sorted(set(words) & GENDERED)
                if gendered and len(set(words)) > 1:
                    labels.add('pronoun ' + '|'.join(sorted(set(words))))
            if labels:
                shapes[m.group(1)] = sorted(labels)
    return shapes


def check(root=ROOT):
    """Errors for shapes outside the baseline and baseline entries now fixed."""
    current = found(root)
    baseline = json.loads((root/BASELINE_PATH).read_text())
    errors = []
    for rule, labels in sorted(current.items()):
        new = sorted(set(labels) - set(baseline.get(rule, ())))
        if new:
            errors.append(f'{rule}: new print-blowup shape {", ".join(new)} '
                          '(docs/print-shapes.md)')
    for rule, labels in sorted(baseline.items()):
        fixed = sorted(set(labels) - set(current.get(rule, ())))
        if fixed:
            errors.append(f'{rule}: shape {", ".join(fixed)} is gone; remove it from '
                          'tools/shape_baseline.json')
    return errors


def remaining_exemptions(current, baseline):
    """Keep only existing exemptions that the current grammar still uses."""
    return {rule: sorted(set(labels) & set(current.get(rule, ())))
            for rule, labels in baseline.items() if set(labels) & set(current.get(rule, ()))}


def prune_fixed(root):
    """Delete resolved exemptions while leaving detection of new shapes to hygiene."""
    path = root/BASELINE_PATH
    baseline = json.loads(path.read_text())
    updated = remaining_exemptions(found(root), baseline)
    if updated == baseline:
        return False
    path.write_text(json.dumps(updated, indent=1, sort_keys=True)+'\n')
    return True


if __name__ == '__main__':
    if '--write-baseline' in sys.argv:
        BASELINE.write_text(json.dumps(found(), indent=1, sort_keys=True) + '\n')
    shapes = found()
    for rule, labels in sorted(shapes.items()):
        print(f'{rule}: {", ".join(labels)}')
    print(f'{len(shapes)} productions, {sum(map(len, shapes.values()))} shapes', file=sys.stderr)
