#!/usr/bin/env python3
"""Check model provenance or fetch/verify an optional validation artifact."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import tomllib
import urllib.request

ROOT = Path(__file__).resolve().parent.parent


def load(path):
    return json.loads(path.read_text())


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def require_hash(value, size, label):
    if not isinstance(value, str) or not re.fullmatch(f'[0-9a-f]{{{size}}}', value):
        raise ValueError(f'{label} needs a full immutable {size}-digit hash')


def check_model(root=ROOT):
    pin = load(root/'pins/model.json')
    if pin['version'] != 2 or pin['implementation'] != 'oracle-owned-crates':
        raise ValueError('unsupported model pin format')
    if type(pin['schema_version']) is not int or pin['schema_version'] < 1:
        raise ValueError('model schema must be a positive integer')
    origin = pin['extracted_from']
    require_hash(origin['revision'], 40, 'extraction revision')
    if not origin['repository'].startswith('https://github.com/'):
        raise ValueError('extraction source must use public HTTPS')
    manifest = tomllib.loads((root/'Cargo.toml').read_text())
    if manifest.get('patch') or manifest.get('replace'):
        raise ValueError('the model must use owned crates without overrides')
    if set(pin['packages']) != {'oracle-ir', 'oracle-ir-contract'}:
        raise ValueError('owned model package identities are incomplete')
    lock = tomllib.loads((root/'Cargo.lock').read_text())
    if any(p['name'] in {'mtgish-ast', 'mtgish-ir', 'rust_syntax'} for p in lock['package']):
        raise ValueError('owned model must not depend on MTGish packages')
    for name, package in pin['packages'].items():
        path = package['path']
        if path != f'crates/{name}':
            raise ValueError('owned model path differs from its package identity')
        wanted = {'path': path, 'version': package['version']}
        if manifest['dependencies'].get(name) != wanted:
            raise ValueError('Cargo dependency differs from the owned model pin')
        crate = tomllib.loads((root/path/'Cargo.toml').read_text())
        if crate.get('patch') or crate.get('replace'):
            raise ValueError('the model must use owned crates without overrides')
        if (crate['package']['name'] != name
                or crate['package']['version'] != package['version']):
            raise ValueError('owned model manifest identity differs')
        records = [p for p in lock['package'] if p['name'] == name]
        if (len(records) != 1 or records[0].get('source')
                or records[0]['version'] != package['version'] or 'replace' in records[0]):
            raise ValueError(f'Cargo.lock differs from the owned {name}')
    return pin


def reference(root=ROOT):
    pin = load(root/'adapters/mtgish/reference.json')
    if pin['version'] != 1:
        raise ValueError('unsupported reference pin format')
    require_hash(pin['revision'], 40, 'reference revision')
    if type(pin['schema_version']) is not int or pin['schema_version'] < 1:
        raise ValueError('reference schema must be a positive integer')
    for kind, format_name in [('source', 'mtgish-source-ron'), ('normalized', 'mtgish-unified-jsonl')]:
        asset = pin[kind]
        require_hash(asset['sha256'], 64, f'{kind} checksum')
        if asset['format'] != format_name:
            raise ValueError(f'unsupported {kind} artifact format')
        if 'url' in asset and not asset['url'].startswith('https://'):
            raise ValueError('reference downloads must use HTTPS')
    require_hash(pin['normalized']['producer_revision'], 40, 'producer revision')
    return pin


def verify(path, kind='normalized', root=ROOT):
    asset = reference(root)[kind]
    if digest(path) != asset['sha256']:
        raise ValueError(f'{kind} artifact checksum mismatch: {path}')
    return path


def fetch(kind='source', root=ROOT):
    pin = reference(root)
    asset = pin[kind]
    if 'url' not in asset:
        raise ValueError('no published download URL; supply the producer output to verify')
    suffix = 'ron' if kind == 'source' else 'jsonl'
    path = root/'.cache/mtgish'/pin['revision']/f'{kind}.{suffix}'
    if path.exists():
        return verify(path, kind, root)
    path.parent.mkdir(parents=True, exist_ok=True)
    # Stream into a temporary file and publish only after checksum verification.
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            with urllib.request.urlopen(asset['url'], timeout=60) as response:
                total = 0
                while chunk := response.read(1024 * 1024):
                    total += len(chunk)
                    if total > 1024 * 1024 * 1024:
                        raise ValueError('reference artifact exceeds 1 GiB')
                    output.write(chunk)
            output.flush()
            verify(temporary, kind, root)
            output.close()
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('check')
    for command in ('verify', 'fetch'):
        child = commands.add_parser(command)
        child.add_argument('--kind', choices=('source', 'normalized'), default='source' if command == 'fetch' else 'normalized')
        if command == 'verify':
            child.add_argument('path', type=Path)
    args = parser.parse_args()
    if args.command == 'check':
        model = check_model()
        ref = reference()
        print(f"owned model schema {model['schema_version']}; extracted from {model['extracted_from']['revision']}")
        print(f"reference schema {ref['schema_version']} at {ref['revision']}")
    elif args.command == 'verify':
        print(verify(args.path, args.kind))
    else:
        print(fetch(args.kind))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, OSError) as error:
        sys.exit(str(error))
