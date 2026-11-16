"""Preview a production-aware merge of an isolated grammar proposal.

Seeded removals are checked against mined.remove and deliberately excluded from
returned changes. The coordinator applies those removals and registered hooks.
"""
from pathlib import Path
import shutil
import tempfile

import mined


class MergeConflict(ValueError):
    """A proposal cannot safely be combined with the current grammar."""


def _blocks(text):
    lines = text.splitlines(keepends=True)
    starts = []
    for index, line, owner, comments in mined.code([line.rstrip('\n') for line in lines]):
        if mined.DEFINITION.match(line):
            starts.append((owner, comments[0] if comments else index, index))
    blocks = {}
    for i, (name, start, definition) in enumerate(starts):
        end = starts[i + 1][1] if i + 1 < len(starts) else len(lines)
        while end > definition + 1 and not lines[end - 1].strip():
            end -= 1
        if name in blocks:
            raise MergeConflict(f'duplicate production {name}')
        blocks[name] = (start, end, ''.join(lines[start:end]))
    return lines, blocks


def _bare(block):
    if block is None or '@sem' in block:
        return None
    lines = block.splitlines()
    entries = mined.definitions(lines)
    if len(entries) != 1:
        return None
    result = mined.alternatives(next(iter(entries.values())))
    return [name for group in result for name in group] if result is not None else None


def _merge_block(name, base, current, proposed):
    if proposed == base or proposed == current:
        return current
    if current == base:
        return proposed
    if base is None or current is None or proposed is None:
        raise MergeConflict(f'conflicting addition or deletion of {name}')
    old, now, new = _bare(base), _bare(current), _bare(proposed)
    if old is not None and now is not None and new is not None:
        # Only reconcile bare dispatcher alternatives; comments are not dispatch data.
        def comments(block):
            lines = block.splitlines(keepends=True)
            return ''.join(lines[:next(i for i, line in enumerate(lines) if mined.DEFINITION.match(line))])
        if comments(base) != comments(current) or comments(base) != comments(proposed):
            raise MergeConflict(f'conflicting comments on {name}')
        remaining = [item for item in now if item not in set(old) - set(new)]
        remaining.extend(item for item in new if item not in old and item not in remaining)
        if not remaining:
            raise MergeConflict(f'dispatcher {name} would be empty')
        prefix = comments(current)
        return prefix + name + ' ::= ' + ' | '.join(remaining) + '\n'
    raise MergeConflict(f'concurrent semantic edits to {name}')


def merge_text(base, current, proposed):
    """Return a production-level three-way merge, or raise MergeConflict."""
    if proposed == base or proposed == current:
        return current
    if current == base:
        # Still validate unique production names in an otherwise trivial merge.
        _blocks(proposed)
        return proposed
    blines, b = _blocks(base)
    clines, c = _blocks(current)
    plines, p = _blocks(proposed)
    # File preambles have no production owner and cannot be combined safely.
    def preamble(lines, blocks):
        start = min((v[0] for v in blocks.values()), default=len(lines))
        return ''.join(lines[:start])
    bp, cp, pp = preamble(blines, b), preamble(clines, c), preamble(plines, p)
    if pp != bp and pp != cp:
        raise MergeConflict('concurrent file preamble edits')
    replacements = []
    additions = []
    for name in dict.fromkeys([*b, *p]):
        before = b.get(name, (0, 0, None))[2]
        present = c.get(name, (0, 0, None))[2]
        after = p.get(name, (0, 0, None))[2]
        merged = _merge_block(name, before, present, after)
        if name in c and merged != present:
            replacements.append((c[name][0], c[name][1], merged or ''))
        elif name not in c and merged is not None:
            additions.append(merged)
    for start, end, replacement in sorted(replacements, reverse=True):
        clines[start:end] = [replacement]
    result = ''.join(clines)
    for block in additions:
        result = result.rstrip('\n') + '\n\n' + block.rstrip('\n') + '\n'
    return result


def merge_grammar(base_dir, current_dir, proposed_dir, assigned_rules, hook_requests):
    """Preview changes as {relative path: text}; do not modify any input tree.

    Hook requests are intentionally not executed here. Seeded files must exactly
    match the output of canonical removal against base and are never returned.
    """
    base, current, proposed = map(Path, (base_dir, current_dir, proposed_dir))
    if not isinstance(hook_requests, list):
        raise MergeConflict('hook requests must be a list')
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        shutil.copytree(base / 'grammar', work / 'grammar')
        try:
            removed, kept = mined.remove(work, assigned_rules)
        except ValueError as exc:
            raise MergeConflict(f'invalid seeded removal: {exc}') from exc
        if set(removed) != set(assigned_rules) or kept:
            raise MergeConflict('assigned seeded rules cannot all be removed')
        for name in ('syntax/mined.ebnf', 'tail/mined.ebnf'):
            expected, actual = work / 'grammar' / name, proposed / 'grammar' / name
            if expected.exists() != actual.exists() or (expected.exists() and expected.read_bytes() != actual.read_bytes()):
                raise MergeConflict(f'noncanonical seeded file: {name}')
    paths = set()
    for root in (base, current, proposed):
        paths.update(path.relative_to(root) for path in (root / 'grammar').rglob('*.ebnf'))
    result, seen, existing = {}, {}, {}
    for path in paths:
        if path.name == 'mined.ebnf' and path.parent.name in ('syntax', 'tail'):
            continue
        file = current / path
        if file.exists():
            for name in _blocks(file.read_text())[1]:
                existing.setdefault(name, set()).add(path)
    for path in sorted(paths):
        if path.name == 'mined.ebnf' and path.parent.name in ('syntax', 'tail'):
            continue
        def read(root):
            file = root / path
            return file.read_text() if file.exists() else ''
        old, now, new = read(base), read(current), read(proposed)
        try:
            merged = merge_text(old, now, new)
            for name in _blocks(merged)[1]:
                seen.setdefault(name, set()).add(path)
        except MergeConflict as exc:
            raise MergeConflict(f'{path}: {exc}') from exc
        if merged != now:
            result[str(path)] = merged
    for name, locations in seen.items():
        if len(locations) > 1 and not locations <= existing.get(name, set()):
            raise MergeConflict(f'new duplicate production {name}: {sorted(map(str, locations))}')
    return result
