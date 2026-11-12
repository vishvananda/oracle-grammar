#!/usr/bin/env python3
"""Remove or adopt seeded (m_ and t_) rules, keeping comments and hooks consistent.

Removal drops a rule with its comments and every bare alternative naming it.

A rule stays when a kept rule uses it in a sequence or as its only alternative; those are
reported, never broken. A hook left with no alternative falls back to its `never_` rule.

Adoption copies a seeded rule into a syntax module under a descriptive name, offered by a
hand-written list; a rule audit then retires the seeded original.
"""
import argparse
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parent.parent
DEFINITION = re.compile(r'([A-Za-z_]\w*) ::= (.*)$')
CONTINUED = re.compile(r'(\s+)\|(.*)')
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


def definitions(lines):
    """Each definition's (index, line) pairs outside comments, by owner."""
    found = {}
    for index, line, owner, _ in code(lines):
        if owner is not None:
            found.setdefault(owner, []).append((index, line))
    return found


def alternatives(entries):
    """A definition's bare alternative names, line by line, or None.

    Only the definition line and continuation lines that begin with `|` qualify, so a
    sequence that wraps onto another line is never split.
    """
    names = []
    for position, (_, line) in enumerate(entries):
        match = (CONTINUED if position else DEFINITION).fullmatch(line)
        if not match:
            return None
        parts = [part.strip() for part in match.group(match.lastindex).split('|')]
        if not all(BARE.fullmatch(part) for part in parts):
            return None
        names.append(parts)
    return names


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
            for owner, entries in definitions(lines).items():
                if owner in removing:
                    continue
                names = alternatives(entries)
                if names is not None and (fallback(owner) or set().union(*names) - removing):
                    continue
                for _, line in entries:
                    for used in uses(line):
                        if used in removing:
                            removing.discard(used)
                            kept[used] = owner
                            changed = True
    return kept


def rewritten(owner, entries, names, removing):
    """The definition without removed alternatives, wrapped as it was."""
    left = [[name for name in line if name not in removing] for line in names]
    indents = [CONTINUED.fullmatch(line).group(1) for _, line in entries[1:]]
    kept = [(indent, line) for indent, line in zip(['', *indents], left) if line]
    if not kept:
        kept = [('', [fallback(owner)])]
    text = [f'{owner} ::= ' + ' | '.join(kept[0][1])]
    text += [(indent or '    ') + '| ' + ' | '.join(line) for indent, line in kept[1:]]
    return '\n'.join(text)


def rewrite(lines, removing):
    """Drop removed definitions and their comments; a gap they leave keeps one blank line."""
    drop, replace = set(), {}
    for index, _, owner, comment in code(lines):
        if owner in removing:
            drop.update([*comment, index])
    for owner, entries in definitions(lines).items():
        names = alternatives(entries)
        if owner in removing or names is None or not removing & set().union(*names):
            continue
        replace[entries[0][0]] = rewritten(owner, entries, names, removing)
        drop.update(index for index, _ in entries[1:])
    out, gap = [], False
    for index, line in enumerate(lines):
        if index in drop:
            gap = True
            continue
        line = replace.get(index, line)
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
        for owner, entries in definitions(lines).items():
            names = alternatives(entries)
            if (owner not in removing and names is not None
                    and not set().union(*names) - removing and fallback(owner) not in defined):
                raise ValueError(f'{owner} would lose every alternative and has no never_ rule')
    for path, lines in texts.items():
        out = rewrite(lines, removing)
        if out != lines:
            path.write_text('\n'.join(out))
    return sorted(removing), kept


PROVENANCE = re.compile(r'/\* \d+ cards?, level:')


def wrapped(names, width=100):
    """Alternative names joined by ` | `, broken into lines of about `width`."""
    lines = [[]]
    for name in names:
        if lines[-1] and len(' | '.join([*lines[-1], name])) > width:
            lines.append([])
        lines[-1].append(name)
    return [' | '.join(line) for line in lines]


def adopt(root, plan):
    """Copy seeded rules into syntax modules as named productions.

    Each entry gives the seeded `rule`, its new `name`, the `module` (a syntax file stem)
    to append it to, the hand-written `parent` list that offers it and a `comment`. The
    copy keeps the rule's `@sem` and other comments but not its provenance. The seeded
    original stays; a rule audit then shows whether it is still needed.
    """
    texts = {path: path.read_text().split('\n') for path in grammar_files(root)}
    found = {owner: (path, entries) for path, lines in texts.items()
             for owner, entries in definitions(lines).items()}
    blocks, offered = {}, {}
    for entry in plan:
        rule, name, parent = entry['rule'], entry['name'], entry['parent']
        target = root/'grammar/syntax'/f"{entry['module']}.ebnf"
        if not SEEDED.fullmatch(rule) or rule not in found:
            raise ValueError(f'{rule} is not a seeded rule')
        if SEEDED.fullmatch(name) or name in found or name in blocks or not BARE.fullmatch(name):
            raise ValueError(f'{name} is not a free production name')
        if target not in texts or target.parent.name != 'syntax':
            raise ValueError(f'no syntax module {entry["module"]}')
        if (SEEDED.fullmatch(parent) or parent not in found
                or alternatives(found[parent][1]) is None):
            raise ValueError(f'{parent} is not a hand-written list of alternatives')
        path, entries = found[rule]
        lines = texts[path]
        body = [DEFINITION.match(entries[0][1]).group(2), *(line for _, line in entries[1:])]
        if any(uses(line) for line in body):
            raise ValueError(f'{rule} uses seeded rules')
        comment = next(comment for index, _, owner, comment in code(lines)
                       if index == entries[0][0])
        kept = [lines[at] for at in comment if not PROVENANCE.match(lines[at])]
        first = DEFINITION.sub(lambda match: f'{name} ::= {match.group(2)}', entries[0][1])
        blocks[name] = (target, [f"/* {entry['comment']} */", *kept, first,
                                 *(line for _, line in entries[1:])])
        offered.setdefault(parent, []).append(name)
    for parent, names in offered.items():
        path, entries = found[parent]
        indents = [CONTINUED.fullmatch(line).group(1) for _, line in entries[1:]]
        texts[path][entries[-1][0]] += ''.join(f'\n{indents[-1] if indents else "    "}| ' + line
                                               for line in wrapped(names))
    for target, block in blocks.values():
        lines = texts[target]
        while lines and not lines[-1].strip():
            lines.pop()
        texts[target] = [*lines, '', *block, '']
    for path, lines in texts.items():
        if lines != path.read_text().split('\n'):
            path.write_text('\n'.join(lines))
    return sorted(blocks)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    commands = parser.add_subparsers(dest='command', required=True)
    removal = commands.add_parser('remove', help='remove seeded rules')
    removal.add_argument('rules', nargs='*')
    removal.add_argument('--unused', type=Path,
                         help="a rule-audit report whose 'unused' rules to remove")
    adoption = commands.add_parser('adopt', help='copy seeded rules into syntax modules')
    adoption.add_argument('plan', type=Path,
                          help='JSON list of {rule, name, module, parent, comment}')
    args = parser.parse_args()
    if args.command == 'adopt':
        print(json.dumps(adopt(ROOT, json.loads(args.plan.read_text()))))
        return
    names = list(args.rules)
    if args.unused:
        names += json.loads(args.unused.read_text())['unused']
    removed, kept = remove(ROOT, names)
    print(json.dumps({'removed': len(removed), 'kept': kept}))


if __name__ == '__main__':
    main()
