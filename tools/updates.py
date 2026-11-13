#!/usr/bin/env python3
"""Recorded updates to the pinned MTGish reference export.

The export stays byte-identical to its pinned checksum. Each update names the faces it
changes, a path inside a face's ability list, the value expected there and its
replacement. Preparing corpus inputs applies every change or fails: a missing face, an
ambiguous face name or a path whose value no longer matches makes the update stale
against the pinned export. A value that already reads `to` was adopted upstream, and the
change should be dropped. Updates are written against one reference revision; advancing
the reference requires reviewing them again.
"""
import json
from pathlib import Path
import re

import pin

UPDATES = pin.ROOT/'adapters/mtgish/updates.json'
KINDS = ('correction', 'normalization')
FIELDS = ('id', 'kind', 'summary', 'evidence', 'changes')


def load(path=None, revision=None):
    """The validated update record; `revision` defaults to the pinned reference's."""
    path = UPDATES if path is None else Path(path)
    data = json.loads(path.read_text())
    if data.get('version') != 1 or set(data) != {'version', 'reference', 'updates'}:
        raise ValueError('unsupported export update format')
    revision = pin.reference()['revision'] if revision is None else revision
    if data['reference'] != revision:
        raise ValueError(f'export updates were written against {data["reference"]}, '
                         f'not the pinned reference {revision}; review them again')
    seen = set()
    for update in data['updates']:
        if set(update) != set(FIELDS):
            raise ValueError(f'an export update needs exactly {", ".join(FIELDS)}')
        if not re.fullmatch(r'U\d{4}', update['id']) or update['id'] in seen:
            raise ValueError(f'export update ids are unique and look like U0001: {update["id"]}')
        seen.add(update['id'])
        if update['kind'] not in KINDS:
            raise ValueError(f'{update["id"]}: kind is one of {", ".join(KINDS)}')
        if not update['summary'].strip() or not update['evidence'].strip():
            raise ValueError(f'{update["id"]}: summary and evidence are required')
        if not update['changes']:
            raise ValueError(f'{update["id"]}: an update changes at least one face')
        for change in update['changes']:
            check_change(update['id'], change)
    return data


def check_change(identity, change):
    if set(change) != {'face', 'path', 'from', 'to'}:
        raise ValueError(f'{identity}: a change needs exactly face, path, from and to')
    path = change['path']
    if not isinstance(change['face'], str) or not change['face']:
        raise ValueError(f'{identity}: a change names its face')
    if not path or not all(type(step) is int and step >= 0 or isinstance(step, str)
                           for step in path):
        raise ValueError(f'{identity}: {change["face"]} needs a path of keys and indexes')
    if change['from'] == change['to']:
        raise ValueError(f'{identity}: {change["face"]} changes nothing')


def pending(data):
    """Changes by face name, in record order."""
    by_face = {}
    for update in data['updates']:
        for change in update['changes']:
            by_face.setdefault(change['face'], []).append((update['id'], change))
    return by_face


def apply(face, changes):
    """Apply one face's changes in place; the ids applied, in order."""
    applied = []
    for identity, change in changes:
        *steps, last = change['path']
        parent = face['abilities']
        try:
            for step in steps:
                parent = parent[step]
            current = parent[last]
        except (KeyError, IndexError, TypeError):
            raise ValueError(f'{identity}: {face["name"]} has no value at {change["path"]}; '
                             'the update is stale') from None
        if current == change['to']:
            raise ValueError(f'{identity}: {face["name"]} already reads the update; '
                             'it was adopted upstream, so drop the change')
        if current != change['from']:
            raise ValueError(f'{identity}: {face["name"]} at {change["path"]} no longer reads '
                             'the recorded value; the update is stale')
        parent[last] = change['to']
        applied.append(identity)
    return applied


def main():
    data = load()
    faces = sum(len(update['changes']) for update in data['updates'])
    print(f'{len(data["updates"])} export updates, {faces} face changes, '
          f'against {data["reference"]}')


if __name__ == '__main__':
    import sys
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        sys.exit(str(error))
