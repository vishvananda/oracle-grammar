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
    if pin['version'] != 1 or pin['implementation'] != 'mtgish-compatibility-bridge':
        raise ValueError('unsupported model pin format')
    require_hash(pin['revision'], 40, 'model revision')
    if type(pin['schema_version']) is not int or pin['schema_version'] < 1:
        raise ValueError('model schema must be a positive integer')
    manifest = tomllib.loads((root/'Cargo.toml').read_text())
    if manifest.get('patch') or manifest.get('replace'):
        raise ValueError('the model must use published source without overrides')
    wanted = {'git': pin['repository'], 'rev': pin['revision'], 'default-features': False}
    if manifest['dependencies']['mtgish-ast'] != wanted:
        raise ValueError('Cargo dependency differs from the model pin')
    if not pin['repository'].startswith('https://github.com/'):
        raise ValueError('model source must use public HTTPS')
    if set(pin['packages']) != {'mtgish-ast', 'mtgish-ir', 'rust_syntax'}:
        raise ValueError('model package identities are incomplete')
    lock = tomllib.loads((root/'Cargo.lock').read_text())
    source = f"git+{pin['repository']}?rev={pin['revision']}#{pin['revision']}"
    for name, version in pin['packages'].items():
        records = [p for p in lock['package'] if p['name'] == name]
        if (len(records) != 1 or records[0].get('source') != source
                or records[0]['version'] != version or 'replace' in records[0]):
            raise ValueError(f'Cargo.lock differs from the pinned {name}')
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
        print(f"model schema {model['schema_version']} at {model['revision']}")
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
