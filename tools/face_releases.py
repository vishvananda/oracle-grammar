"""Export pinned source with an exact code tag, independently of exporter changes."""
from contextlib import contextmanager
import gzip
import json
from pathlib import Path
import re
import subprocess
import tempfile

import releases


def source_config(source):
    if source.get('format_version') != 1 or source.get('provider') != 'scryfall':
        raise ValueError('expected source manifest format 1, provider scryfall')
    releases.slug(source.get('snapshot_id'))
    releases.sha(source.get('sha256'))
    if not str(source.get('url', '')).startswith('https://'):
        raise ValueError('source URL must use HTTPS')
    if source.get('encoding', 'json') not in ('json', 'jsonl-gzip'):
        raise ValueError('unsupported Oracle source encoding')
    scope = source.get('population', {})
    if not isinstance(scope.get('policy'), str) or not scope['policy'].strip():
        raise ValueError('explicit population policy required')
    if scope.get('kind') not in ('all', 'oracle_ids'):
        raise ValueError('population must explicitly select all or oracle_ids')
    if scope['kind'] == 'oracle_ids':
        ids = scope.get('oracle_ids')
        if not isinstance(ids, list) or not ids or any(not isinstance(i, str) or not i for i in ids) or len(ids) != len(set(ids)):
            raise ValueError('oracle_ids must be nonempty, distinct strings')


def tag_for(spec, meta):
    source_config(spec.get('source', {}))
    revision = spec.get('revision')
    if spec.get('format_version') != 1 or not isinstance(revision, int) or isinstance(revision, bool) or revision < 1:
        raise ValueError('release specification requires format 1 and a positive integer revision')
    if spec.get('grammar_tag') != 'grammar-v'+meta['grammar']['version']:
        raise ValueError('grammar tag and package version disagree')
    return (f"faces-oracle-{spec['source']['snapshot_id']}-s{meta['model']['schema_version']}"
            f"-g{meta['grammar']['version']}-r{revision}")


@contextmanager
def producer(tag, root):
    if not re.fullmatch(r'grammar-v\d+\.\d+\.\d+', tag or ''):
        raise ValueError('expected an immutable grammar-v<VERSION> tag')
    commit = releases.run('git', 'rev-parse', 'refs/tags/'+tag+'^{commit}', cwd=root)
    with tempfile.TemporaryDirectory() as directory:
        checkout = Path(directory)/'producer'
        subprocess.run(['git', 'worktree', 'add', '--detach', str(checkout), commit], cwd=root, check=True)
        try:
            meta = releases.metadata(checkout)
            if tag != 'grammar-v'+meta['grammar']['version']:
                raise ValueError('grammar tag and package version disagree')
            yield checkout, meta
        finally:
            subprocess.run(['git', 'worktree', 'remove', '--force', str(checkout)], cwd=root, check=True)


def specification(tag=None, grammar_tag=None, root=releases.ROOT, meta=None):
    if bool(tag) == bool(grammar_tag):
        raise ValueError('specify exactly one face tag or grammar tag')
    if tag:
        releases.slug(tag)
        path = root/'releases'/'faces'/(tag+'.json')
    else:
        path = root/'releases'/'oracle-source.json'
    spec = json.loads(path.read_text())
    if grammar_tag:
        spec['grammar_tag'] = grammar_tag
    if meta is None:
        with producer(spec.get('grammar_tag'), root) as (_, meta):
            expected = tag_for(spec, meta)
    else:
        expected = tag_for(spec, meta)
    if tag and tag != expected:
        raise ValueError('face tag, grammar tag and release specification disagree')
    return spec


def records(source, spec):
    if spec.get('encoding', 'json') not in ('json', 'jsonl-gzip'):
        raise ValueError('unsupported Oracle source encoding')
    if releases.digest(source) != spec['sha256']:
        raise ValueError('Oracle source checksum mismatch')
    if spec.get('encoding', 'json') == 'jsonl-gzip':
        with gzip.open(source, 'rt', encoding='utf-8') as stream:
            values = [json.loads(line) for line in stream if line.strip()]
    else:
        values = json.loads(Path(source).read_text())
    if not isinstance(values, list) or not values or any(not isinstance(r, dict) for r in values):
        raise ValueError('source must contain a nonempty Oracle record array')
    if spec['population']['kind'] == 'oracle_ids':
        wanted = set(spec['population']['oracle_ids'])
        values = [r for r in values if r.get('oracle_id') in wanted]
        if {r.get('oracle_id') for r in values} != wanted:
            raise ValueError('population includes missing source records')
    return values


def export(out, *, tag=None, grammar_tag=None, root=releases.ROOT):
    exporter = releases.metadata(root)
    spec = specification(tag, grammar_tag, root)
    with producer(spec['grammar_tag'], root) as (checkout, meta), tempfile.TemporaryDirectory() as directory:
        directory = Path(directory)
        source = directory/'oracle.source'
        releases.fetch(spec['source']['url'], source, spec['source']['sha256'])
        selected = records(source, spec['source'])
        expected = sum(len(r.get('card_faces', [r])) for r in selected)
        input_path, output_path = directory/'selected.json', directory/'outcomes.jsonl'
        input_path.write_bytes(releases.json_bytes(selected))
        binaries = releases.build(checkout)
        subprocess.run([str(binaries/'native-export'), str(input_path), str(output_path)],
                       cwd=directory, env=releases.environment(), check=True)
        rows = [json.loads(line) for line in output_path.read_text().splitlines()]
        if len(rows) != expected:
            raise ValueError('missing input face outcomes')
        releases.unchanged(meta, checkout)
        releases.unchanged(exporter, root)
        releases.bundle(out, rows, meta, spec['source'], spec['revision'], exporter['grammar']['commit'], source)
        if releases.verify(out)['tag'] != tag_for(spec, meta):
            raise ValueError('export identity differs from requested tag')
