#!/usr/bin/env python3
"""Reproducible grammar packages and native face bundles; publication is explicit."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import urllib.request
import urllib.error

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ('oracle-ir', 'oracle-ir-contract', 'oracle-ir-grammar')


def run(*args, cwd=ROOT, **kwargs):
    return subprocess.check_output(args, cwd=cwd, text=True, **kwargs).strip()


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b''):
            value.update(chunk)
    return value.hexdigest()


def json_bytes(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(',', ':'))+'\n').encode()


def sha(value):
    if not isinstance(value, str) or not re.fullmatch('[0-9a-f]{64}', value):
        raise ValueError('expected a lowercase SHA-256')
    return value


def slug(value):
    if not isinstance(value, str) or not re.fullmatch('[A-Za-z0-9][A-Za-z0-9._-]{0,127}', value):
        raise ValueError('invalid release identity component')
    return value


def metadata(root=ROOT, clean=True):
    if clean and run('git', 'status', '--porcelain', cwd=root):
        raise ValueError('release requires a clean checkout, including untracked files')
    commit = run('git', 'rev-parse', 'HEAD', cwd=root)
    manifests = [root/'crates'/p/'Cargo.toml' for p in PACKAGES[:-1]]+[root/'Cargo.toml']
    versions = {p: tomllib.loads(path.read_text())['package']['version'] for p, path in zip(PACKAGES, manifests)}
    version = versions['oracle-ir-grammar']
    if not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('release version must be plain SemVer')
    inputs = {str(p.relative_to(root)): digest(p) for p in sorted((root/'grammar').rglob('*')) if p.is_file()}
    pin = json.loads((root/'pins/model.json').read_text())
    return {'grammar': {'crate': 'oracle-ir-grammar', 'version': version, 'commit': commit,
                        'inputs_sha256': hashlib.sha256(json_bytes(inputs)).hexdigest()},
            'grammar_inputs': inputs, 'model': {'schema_version': pin['schema_version'], 'packages': versions},
            'toolchain': {'rustc': run('rustc', '-V'), 'cargo': run('cargo', '-V')}}


def unchanged(meta, root=ROOT):
    if metadata(root) != meta:
        raise ValueError('release checkout changed during artifact preparation')


def fresh(path):
    path = Path(path).resolve()
    path.mkdir(parents=True, exist_ok=False)
    return path


def finish(out, manifest):
    manifest['assets'] = {p.name: {'sha256': digest(p), 'bytes': p.stat().st_size}
                          for p in sorted(out.iterdir()) if p.is_file()}
    (out/'manifest.json').write_bytes(json_bytes(manifest))
    (out/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in sorted(out.iterdir()) if p.is_file()))
    verify(out)


def verify(out):
    out = Path(out)
    manifest = json.loads((out/'manifest.json').read_text())
    sums = {}
    for line in (out/'SHA256SUMS').read_text().splitlines():
        checksum, name = line.split('  ', 1)
        if Path(name).name != name or name in sums:
            raise ValueError('invalid checksum asset name')
        if digest(out/name) != sha(checksum):
            raise ValueError(f'checksum mismatch: {name}')
        sums[name] = checksum
    if set(sums) != set(manifest['assets']) | {'manifest.json'}:
        raise ValueError('manifest/checksum inventory differs')
    if {p.name for p in out.iterdir()} != set(sums) | {'SHA256SUMS'}:
        raise ValueError('unlisted release assets')
    for name, asset in manifest['assets'].items():
        if sums[name] != sha(asset['sha256']) or (out/name).stat().st_size != asset['bytes']:
            raise ValueError(f'manifest asset mismatch: {name}')
    return manifest


def archive(path, files, root=ROOT):
    with path.open('wb') as file:
        with gzip.GzipFile(filename='', mode='wb', fileobj=file, mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode='w') as tar:
                for source in sorted(files):
                    data = source.read_bytes()
                    info = tarfile.TarInfo(str(source.relative_to(root)))
                    info.size, info.mode, info.mtime = len(data), 0o644, 0
                    tar.addfile(info, io.BytesIO(data))


def environment():
    env = dict(os.environ)
    # Release meaning must use exactly the checkout's locked, embedded rules.
    for key in ('ORACLE_GRAMMAR_DIR', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS'):
        if env.get(key):
            raise ValueError(f'release must not override {key}')
    return env


def build():
    env = environment()
    subprocess.run(['cargo', 'build', '--locked', '--release', '--features', 'cli,embedded-grammar',
                    '--bin', 'native-export', '--bin', 'grammar-export', '--bin', 'family-check'], cwd=ROOT, env=env, check=True)
    info = json.loads(run('cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'))
    return Path(info['target_directory'])/'release'


def native_smoke(binaries):
    fixture = [{'oracle_id': 'synthetic-draw', 'name': 'Synthetic draw', 'layout': 'normal',
                'type_line': 'Instant', 'mana_cost': '{U}', 'oracle_text': 'Draw a card.'},
               {'oracle_id': 'synthetic-refusal', 'name': 'Synthetic refusal', 'layout': 'normal',
                'type_line': 'Instant', 'oracle_text': 'Nonsense not in Oracle.'}]
    with tempfile.TemporaryDirectory() as directory:
        source, result = Path(directory)/'source.json', Path(directory)/'outcomes.jsonl'
        source.write_bytes(json_bytes(fixture))
        subprocess.run([str(binaries/'native-export'), str(source), str(result)],
                       cwd=directory, env=environment(), check=True)
        rows = [json.loads(line) for line in result.read_text().splitlines()]
    if [r['status'] for r in rows] != ['unique', 'refused']:
        raise ValueError('packaged native CLI recognition/refusal smoke test failed')


def code(out, tag):
    meta = metadata()
    if tag != 'grammar-v'+meta['grammar']['version']:
        raise ValueError('code tag must match grammar package version')
    env = environment()
    subprocess.run(['cargo', 'package', '--locked', '--workspace'], cwd=ROOT, env=env, check=True)
    binaries = build()
    families = json.loads((ROOT/'tests/families.json').read_text())
    subprocess.run([str(binaries/'family-check'), *families, '--compositional'],
                   cwd=ROOT, env=env, check=True)
    native_smoke(binaries)
    target = binaries.parent
    out = fresh(out)
    for package, version in meta['model']['packages'].items():
        name = f'{package}-{version}.crate'
        shutil.copyfile(target/'package'/name, out/name)
    archive(out/'grammar-sources.tar.gz', [p for p in (ROOT/'grammar').rglob('*') if p.is_file()]+[ROOT/'LICENSE-MIT', ROOT/'LICENSE-APACHE'])
    with (out/'grammar.ebnf').open('w') as file:
        subprocess.run([str(binaries/'grammar-export')], cwd=out, stdout=file, env=env, check=True)
    # Executables are omitted until platform archives and their licenses are shipped.
    unchanged(meta)
    finish(out, {'format_version': 1, 'kind': 'oracle-grammar-code', 'tag': tag, **meta,
                 'build': {'locked': True, 'features': ['cli', 'embedded-grammar']}})


def source_spec(path, source):
    spec = json.loads(Path(path).read_text())
    if spec.get('format_version') != 1 or spec.get('provider') != 'scryfall':
        raise ValueError('expected source manifest format 1, provider scryfall')
    slug(spec.get('snapshot_id'))
    if digest(source) != sha(spec.get('sha256')):
        raise ValueError('Oracle source checksum mismatch')
    scope = spec.get('population', {})
    if not isinstance(scope.get('policy'), str) or not scope['policy'].strip():
        raise ValueError('explicit population policy required')
    if scope.get('kind') not in ('all', 'oracle_ids'):
        raise ValueError('population must explicitly select all or oracle_ids')
    if scope['kind'] == 'oracle_ids':
        ids = scope.get('oracle_ids')
        if not isinstance(ids, list) or not ids or any(not isinstance(i, str) or not i for i in ids) or len(ids) != len(set(ids)):
            raise ValueError('oracle_ids must be nonempty, distinct strings')
    return spec


def gzip_rows(path, rows):
    with path.open('wb') as file:
        with gzip.GzipFile(filename='', mode='wb', fileobj=file, mtime=0) as zipped:
            for row in rows:
                zipped.write(json_bytes(row))


def bundle(out, rows, meta, spec, revision, exporter_commit=None):
    if revision < 1:
        raise ValueError('export revision must be positive')
    identities = [json_bytes(row['identity']) for row in rows]
    if len(identities) != len(set(identities)):
        raise ValueError('duplicate source occurrence identity')
    counts = {key: 0 for key in ('unique', 'ambiguous', 'refused', 'limit')}
    for row in rows:
        if row['schema_version'] != meta['model']['schema_version'] or row['format_version'] != 1:
            raise ValueError('outcome schema/container mismatch')
        status = row['status']
        if status not in counts:
            raise ValueError('unknown outcome status')
        readings = row['readings']
        cards = {json_bytes(r['card']) for r in readings}
        expected = 'unique' if len(cards) == 1 else 'ambiguous' if len(cards) > 1 else None
        if status in ('unique', 'ambiguous') and (expected != status or row['semantic_readings'] != len(cards)):
            raise ValueError('success status disagrees with distinct reading count')
        if status in ('refused', 'limit') and readings:
            raise ValueError('refusal cannot contain partial success')
        counts[status] += 1
    rows = sorted(rows, key=lambda row: (row['identity']['record_index'], row['identity']['face_index']))
    out = fresh(out)
    gzip_rows(out/'outcomes.jsonl.gz', rows)
    gzip_rows(out/'faces.jsonl.gz', (r for r in rows if r['status'] == 'unique'))
    coverage = {'input_faces': len(rows), 'counts': counts,
                'gates': {'whole_input_native_parse': True, 'typed_validation': True,
                          'reference_agreement': 'not_run', 'print_reparse': 'not_run', 'exact_wording': 'not_run'}}
    (out/'coverage.json').write_bytes(json_bytes(coverage))
    tag = f"faces-oracle-{spec['snapshot_id']}-s{meta['model']['schema_version']}-g{meta['grammar']['version']}-r{revision}"
    finish(out, {'format_version': 1, 'kind': 'oracle-grammar-native-faces', 'tag': tag, **meta,
                 'source_oracle': {k: spec[k] for k in ('provider', 'snapshot_id', 'sha256')},
                 'population': {**spec['population'], 'sha256': hashlib.sha256(json_bytes(spec['population'])).hexdigest()},
                 'exporter': {'commit': exporter_commit or meta['grammar']['commit'], 'revision': revision},
                 'options': {'source_format': 'scryfall-oracle-json', 'features': ['cli', 'embedded-grammar'],
                             'parser_limits': 'crate defaults', 'all_readings': True}, 'counts': coverage})


def faces(source, spec_path, out, revision):
    meta = metadata()
    producer = json.loads(json.dumps(meta))
    # Data releases can only use a reviewed, immutable code-release commit.
    tag = 'grammar-v'+meta['grammar']['version']
    grammar_commit = run('git', 'rev-parse', tag+'^{commit}')
    exporter_commit = meta['grammar']['commit']
    compiled_inputs = ('Cargo.toml', 'Cargo.lock', 'src', 'crates', 'grammar',
                       'pins/model.json', 'rust-toolchain.toml', '.cargo/config.toml')
    changed = subprocess.run(['git', 'diff', '--quiet', grammar_commit, exporter_commit,
                              '--', *compiled_inputs], cwd=ROOT)
    if changed.returncode:
        raise ValueError('compiled grammar differs from code release; release a new crate version')
    meta['grammar']['commit'] = grammar_commit
    spec = source_spec(spec_path, source)
    records = json.loads(Path(source).read_text())
    if not isinstance(records, list):
        raise ValueError('source must be an Oracle bulk JSON array')
    scope = spec['population']
    if scope['kind'] == 'oracle_ids':
        wanted = set(scope['oracle_ids'])
        records = [record for record in records if record.get('oracle_id') in wanted]
        if {r['oracle_id'] for r in records} != wanted:
            raise ValueError('population includes missing source records')
    expected = sum(len(record.get('card_faces', [record])) for record in records)
    binaries = build()
    with tempfile.TemporaryDirectory() as directory:
        selected, output = Path(directory)/'selected.json', Path(directory)/'outcomes.jsonl'
        selected.write_bytes(json_bytes(records))
        subprocess.run([str(binaries/'native-export'), str(selected), str(output)], cwd=directory,
                       env=environment(), check=True)
        rows = [json.loads(line) for line in output.read_text().splitlines()]
    if len(rows) != expected:
        raise ValueError('missing input face outcomes')
    unchanged(producer)
    bundle(out, rows, meta, spec, revision, exporter_commit)


def registry_checksum(package, version):
    if not re.fullmatch(r'[a-z0-9-]+', package) or not re.fullmatch(r'\d+\.\d+\.\d+', version):
        raise ValueError('invalid crate identity')
    request = urllib.request.Request(f'https://crates.io/api/v1/crates/{package}/{version}',
                                     headers={'User-Agent': 'oracle-grammar-release/1'})
    try:
        with urllib.request.urlopen(request) as response:
            return json.load(response)['version']['checksum']
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        raise


def publish_crates(out):
    manifest = verify(out)
    if manifest['kind'] != 'oracle-grammar-code' or metadata()['grammar'] != manifest['grammar']:
        raise ValueError('publish must use the exact clean code release checkout')
    for package in PACKAGES:
        version = manifest['model']['packages'][package]
        asset = f'{package}-{version}.crate'
        expected = manifest['assets'][asset]['sha256']
        published = registry_checksum(package, version)
        if published:
            if published != expected:
                raise ValueError(f'existing {package} {version} differs; use a new version')
            continue
        subprocess.run(['cargo', 'publish', '--locked', '--dry-run', '-p', package], cwd=ROOT, check=True)
        target = Path(json.loads(run('cargo', 'metadata', '--locked', '--no-deps', '--format-version=1'))['target_directory'])
        if digest(target/'package'/asset) != expected:
            raise ValueError(f'dry-run package differs from reviewed release: {asset}')
        subprocess.run(['cargo', 'publish', '--locked', '-p', package], cwd=ROOT, check=True)
        # cargo publish waits for index availability. A retry verifies exact bytes.
        if registry_checksum(package, version) != expected:
            raise ValueError(f'published registry checksum differs: {asset}')


def github(out, repository, draft=True):
    manifest = verify(out)
    tag, commit = manifest['tag'], manifest['grammar']['commit']
    if not re.fullmatch(r'[\w.-]+/[\w.-]+', repository):
        raise ValueError('invalid GitHub repository')
    # Refuse existing releases rather than overwrite assets. Complete or delete
    # an interrupted draft deliberately after checking its uploaded checksums.
    probe = subprocess.run(['gh', 'release', 'view', tag, '--repo', repository], capture_output=True)
    if probe.returncode == 0:
        raise ValueError('release already exists; inspect its assets before retrying')
    notes = f"{manifest['kind']} from {commit}. See manifest.json for versions, source scope and checksums."
    subprocess.run(['gh', 'release', 'create', tag, '--repo', repository, '--target', commit,
                    '--draft', '--title', tag, '--notes', notes,
                    *map(str, sorted(Path(out).iterdir()))], check=True)
    with tempfile.TemporaryDirectory() as directory:
        subprocess.run(['gh', 'release', 'download', tag, '--repo', repository, '--dir', directory], check=True)
        verify(directory)
        if digest(Path(directory)/'manifest.json') != digest(Path(out)/'manifest.json'):
            raise ValueError('uploaded manifest differs')
    if not draft:
        subprocess.run(['gh', 'release', 'edit', tag, '--repo', repository, '--draft=false'], check=True)


def fetch(url, path, checksum):
    if not url.startswith('https://'):
        raise ValueError('release inputs require HTTPS')
    sha(checksum)
    request = urllib.request.Request(url, headers={'User-Agent': 'oracle-grammar-release/1'})
    with urllib.request.urlopen(request) as response, Path(path).open('xb') as file:
        shutil.copyfileobj(response, file)
    if digest(path) != checksum:
        Path(path).unlink()
        raise ValueError('download checksum mismatch')


def face_spec(tag, root=ROOT):
    slug(tag)
    spec = json.loads((root/'releases'/'faces'/(tag+'.json')).read_text())
    if spec.get('format_version') != 1:
        raise ValueError('unsupported face release specification')
    source, revision = spec.get('source', {}), spec.get('revision')
    if not isinstance(revision, int) or isinstance(revision, bool) or revision < 1:
        raise ValueError('release specification requires a positive integer revision')
    meta = metadata(root)
    expected = f"faces-oracle-{slug(source.get('snapshot_id'))}-s{meta['model']['schema_version']}-g{meta['grammar']['version']}-r{revision}"
    if tag != expected or spec.get('grammar_tag') != 'grammar-v'+meta['grammar']['version']:
        raise ValueError('face tag, grammar tag and release specification disagree')
    if not str(source.get('url', '')).startswith('https://'):
        raise ValueError('source URL must use HTTPS')
    sha(source.get('sha256'))
    return spec


def faces_tag(tag, out):
    spec = face_spec(tag)
    with tempfile.TemporaryDirectory() as directory:
        source, manifest = Path(directory)/'oracle.json', Path(directory)/'source.json'
        fetch(spec['source']['url'], source, spec['source']['sha256'])
        manifest.write_bytes(json_bytes(spec['source']))
        faces(source, manifest, out, spec['revision'])
    if verify(out)['tag'] != tag:
        raise ValueError('export identity differs from requested tag')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subs = parser.add_subparsers(dest='command', required=True)
    p = subs.add_parser('code'); p.add_argument('--out', required=True); p.add_argument('--tag', required=True)
    p = subs.add_parser('faces-tag'); p.add_argument('--tag', required=True); p.add_argument('--out', required=True)
    p = subs.add_parser('faces'); p.add_argument('--source', required=True); p.add_argument('--source-manifest', required=True)
    p.add_argument('--out', required=True); p.add_argument('--revision', type=int, default=1)
    p = subs.add_parser('fetch'); p.add_argument('--url', required=True); p.add_argument('--out', required=True); p.add_argument('--sha256', required=True)
    p = subs.add_parser('verify'); p.add_argument('directory')
    p = subs.add_parser('publish-crates'); p.add_argument('directory')
    p = subs.add_parser('github'); p.add_argument('directory'); p.add_argument('--repo', required=True)
    p.add_argument('--publish', action='store_true', help='publish after verifying uploaded draft assets')
    args = parser.parse_args()
    if args.command == 'code': code(args.out, args.tag)
    elif args.command == 'faces-tag': faces_tag(args.tag, args.out)
    elif args.command == 'faces': faces(args.source, args.source_manifest, args.out, args.revision)
    elif args.command == 'fetch': fetch(args.url, args.out, args.sha256)
    elif args.command == 'verify': verify(args.directory)
    elif args.command == 'publish-crates': publish_crates(args.directory)
    elif args.command == 'github': github(args.directory, args.repo, not args.publish)


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
