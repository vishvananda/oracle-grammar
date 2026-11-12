#!/usr/bin/env python3
"""Remove seeded (m_ and t_) rules with their comments and every bare alternative naming them.

A rule stays when a kept rule uses it in a sequence or as its only alternative; those are
reported, never broken. A hook left with no alternative falls back to its `never_` rule.
"""
import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
DEFINITION = re.compile(r'([A-Za-z_]\w*) ::= (.*)$')
SEEDED = re.compile(r'\b[mt]_\w+\b')
QUOTED = re.compile(r'"(?:[^"\\]|\\.)*"')
BARE = re.compile(r'[A-Za-z_]\w*')


def grammar_files(root):
    return sorted([*(root/'grammar/syntax').glob('*.ebnf'), *(root/'grammar/tail').glob('*.ebnf')])


def code(lines):
    """(index, line, owner, comment lines above) for each line outside a comment."""
    owner, comment, inside = None, [], False
    for index, line in enumerate(lines):
        if inside or line.lstrip().startswith('/*'):
            comment.append(index)
            inside = '*/' not in line
            continue
        if not line.strip():
            comment = []
            continue
        match = DEFINITION.match(line)
        if match:
            owner = match.group(1)
        yield index, line, owner, comment
        comment = []


def alternatives(line):
    """The names of a one-line, all-bare alternative list, or None."""
    match = DEFINITION.match(line)
    if not match:
        return None
    parts = [part.strip() for part in match.group(2).split('|')]
    return parts if all(BARE.fullmatch(part) for part in parts) else None


def fallback(rule):
    if rule.startswith('mined_'):
        return 'never_' + rule[len('mined_'):]
    if rule.startswith('tail_'):
        return 'never_' + rule
    return None


def uses(line):
    """Seeded names a line uses outside quoted words and comments."""
    return SEEDED.findall(QUOTED.sub('', re.sub(r'/\*.*?\*/', '', line)))


def keep_used(texts, removing):
    """Move out of `removing` each rule a kept rule cannot lose; map it to that user."""
    kept, changed = {}, True
    while changed:
        changed = False
        for lines in texts.values():
            for _, line, owner, _ in code(lines):
                if owner in removing:
                    continue
                names = alternatives(line)
                if names is not None and (fallback(owner) or set(names) - removing):
                    continue
                for used in uses(line):
                    if used in removing:
                        removing.discard(used)
                        kept[used] = owner
                        changed = True
    return kept


def rewrite(lines, removing):
    """Drop removed definitions and their comments; a gap they leave keeps one blank line."""
    drop = set()
    for index, _, owner, comment in code(lines):
        if owner in removing:
            drop.update([*comment, index])
    out, gap = [], False
    for index, line in enumerate(lines):
        if index in drop:
            gap = True
            continue
        names = alternatives(line)
        if names is not None and removing & set(names):
            rule = DEFINITION.match(line).group(1)
            left = [name for name in names if name not in removing] or [fallback(rule)]
            line = f'{rule} ::= ' + ' | '.join(left)
        if gap and not line.strip() and out and not out[-1].strip():
            continue
        out.append(line)
        gap = False
    return out


def remove(root, names):
    """Remove seeded `names`; return (removed, kept) where kept maps a rule to its user."""
    texts = {path: path.read_text().split('\n') for path in grammar_files(root)}
    defined = set()
    for lines in texts.values():
        for _, line, owner, _ in code(lines):
            if DEFINITION.match(line):
                if owner in defined and SEEDED.fullmatch(owner):
                    raise ValueError(f'{owner} is defined twice')
                defined.add(owner)
    unknown = sorted(name for name in set(names) if name not in defined or not SEEDED.fullmatch(name))
    if unknown:
        raise ValueError('not seeded rules: ' + ', '.join(unknown[:8]))
    removing = set(names)
    kept = keep_used(texts, removing)
    for lines in texts.values():
        for _, line, owner, _ in code(lines):
            names_here = alternatives(line)
            if (owner not in removing and names_here is not None
                    and not set(names_here) - removing and fallback(owner) not in defined):
                raise ValueError(f'{owner} would lose every alternative and has no never_ rule')
    for path, lines in texts.items():
        out = rewrite(lines, removing)
        if out != lines:
            path.write_text('\n'.join(out))
    return sorted(removing), kept


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    commands = parser.add_subparsers(dest='command', required=True)
    removal = commands.add_parser('remove', help='remove seeded rules')
    removal.add_argument('rules', nargs='*')
    removal.add_argument('--unused', type=Path,
                         help="a rule-audit report whose 'unused' rules to remove")
    args = parser.parse_args()
    names = list(args.rules)
    if args.unused:
        names += json.loads(args.unused.read_text())['unused']
    removed, kept = remove(ROOT, names)
    print(json.dumps({'removed': len(removed), 'kept': kept}))


if __name__ == '__main__':
    main()
