#!/usr/bin/env python3
"""Apply the reviewed MTGish Scryfall input exclusions to original Oracle IDs."""
from collections import Counter
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re


UPSTREAM = 'e8faef56112ef0c979f6174acadcfe3e21505dd6'
IGNORE_SHA256 = '77e86963b89f9c3a6799db9fc35f53ecd347f0a9552a1d8fb785084387708a09'

# The upstream input stage loads these items from separate sources or leaves
# them out of the ordinary Oracle import.
SUPPLEMENTAL_LAYOUTS = {
    'art_series', 'emblem', 'token', 'double_faced_token', 'flip_token', 'front_card',
}
FLIP_TOKENS = {
    'Monster // Sorcerer', 'Monster // Virtuous', 'Royal // Virtuous',
    'Royal // Young Hero', 'Sorcerer // Virtuous', 'Wicked // Cursed',
}
SINGLE_LAYOUTS = {
    'normal', 'prototype', 'case', 'class', 'leveler', 'saga', 'mutate',
    'vanguard', 'scheme', 'planar', 'host', 'augment',
}
MULTI_LAYOUTS = {'meld', 'modal_dfc', 'flip', 'transform', 'adventure', 'prepare', 'split'}
ADVENTURE_TYPES = {f'{kind} — {variant}' for kind in ('Instant', 'Sorcery')
                   for variant in ('Adventure', 'Omen')}

# Only for matching the upstream ignore names. Input names and rules text stay
# exactly as Scryfall supplied them.
ALIASES = {
    ('unk', 'Fast // Furious'): 'Fasto // Furiouso',
    ('cmb2', 'Red Herring'): 'Sketchy Red Herring',
    ('cmb2', 'Pick Your Poison'): 'Pick Your Sketchy Poison',
    ('tbth', 'Unquenchable Fury'): "Horde's Unquenchable Fury",
    ('unk', 'Joven and Chandler'): 'Joven and Chandler, Backup Buddies',
    ('punk', 'No Way Out'): 'No Way Out But Through',
    ('punk', 'Artist Alley'): 'Artist Alley at the Convention',
}

JSONC_TOKENS = re.compile(r'"(?:[^"\\]|\\.)*"|//[^\n]*|/\*[\s\S]*?\*/|,|\[|\]|\s+|.')


def read_ignores(path):
    """Read the upstream array of JSON strings with comments and trailing commas."""
    parts = []
    for token in JSONC_TOKENS.findall(path.read_text()):
        if token.startswith(('//', '/*')) or token.isspace():
            continue
        if token == ']' and parts and parts[-1] == ',':
            parts.pop()
        parts.append(token)
    names = json.loads(''.join(parts))
    if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
        raise ValueError('MTGish ignore input must be an array of names')
    return set(names)


def exclusion(card, ignored_names):
    name, layout, types = (card[key] for key in ('name', 'layout', 'type_line'))
    if types == 'Dungeon':
        return 'supplemental_dungeon', None
    if layout in SUPPLEMENTAL_LAYOUTS or name in FLIP_TOKENS:
        return 'excluded_layout', None
    if layout in SINGLE_LAYOUTS and types == 'Card':
        return 'non_game_card', None
    if layout not in SINGLE_LAYOUTS | MULTI_LAYOUTS:
        return 'unhandled_upstream_layout', None
    if layout == 'adventure':
        faces = card.get('card_faces', [])
        if len(faces) != 2:
            raise ValueError(f'adventure layout requires two faces for {name}')
        if faces[1].get('type_line') not in ADVENTURE_TYPES:
            return 'unhandled_upstream_adventure', None
    upstream_name = ALIASES.get((card['set'], name), name)
    if upstream_name == 'Human—Time Lord Meta-Crisis':
        upstream_name = upstream_name.replace('—', '-')
    if upstream_name in ignored_names:
        return 'upstream_ignore_name', upstream_name
    return None, None


def sha256(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def classify(source, ignores, snapshot, expected_sha256):
    if sha256(source) != expected_sha256:
        raise ValueError('Oracle source SHA-256 mismatch')
    if sha256(ignores) != IGNORE_SHA256:
        raise ValueError(f'ignore SHA-256 mismatch for MTGish revision {UPSTREAM}')
    ignored_names = read_ignores(ignores)
    ignored, included, seen = [], [], set()
    with gzip.open(source, 'rt', encoding='utf-8') as stream:
        for line in stream:
            if not line.strip():
                continue
            card = json.loads(line)
            identity = card.get('oracle_id')
            if not isinstance(identity, str) or not identity or identity in seen:
                raise ValueError('Oracle identity must be unique and nonempty')
            seen.add(identity)
            reason, matched_name = exclusion(card, ignored_names)
            if reason is None:
                included.append(identity)
                continue
            row = {'oracle_id': identity, 'name': card['name'], 'layout': card['layout'],
                   'set': card['set'], 'reason': reason}
            if matched_name is not None:
                row['upstream_name'] = matched_name
            ignored.append(row)
    population = {'kind': 'oracle_ids', 'oracle_ids': sorted(included),
                  'policy': f'Original Scryfall Oracle records selected by MTGish {UPSTREAM}'}
    return {'format_version': 1, 'mtgish_revision': UPSTREAM,
            'mtgish_ignore_sha256': IGNORE_SHA256,
            'source': {'snapshot_id': snapshot, 'sha256': expected_sha256},
            'counts': {'source': len(seen), 'included': len(included), 'ignored': len(ignored),
                       'ignore_reasons': dict(Counter(row['reason'] for row in ignored))},
            'ignored': sorted(ignored, key=lambda row: row['oracle_id']),
            'population': population}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path, help='original Scryfall JSONL gzip snapshot')
    parser.add_argument('ignore_names', type=Path, help='pinned MTGish grammars/ignore.json5')
    parser.add_argument('--snapshot', required=True)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    result = classify(args.source, args.ignore_names, args.snapshot, args.sha256)
    with args.out.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, ensure_ascii=False, indent=2)
        stream.write('\n')
    print(json.dumps(result['counts'], sort_keys=True))


if __name__ == '__main__':
    main()
