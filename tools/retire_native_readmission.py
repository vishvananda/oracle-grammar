"""Verify focused isolation proof for a native-failing integration batch.

This evidence only authorizes fresh staging and validation; it is not acceptance.
"""
import hashlib
import json
from pathlib import Path


def _allowed(campaign, path):
    resolved = path.resolve()
    return resolved.is_relative_to((campaign/'.cache').resolve()) or resolved.is_relative_to(Path('/mnt'))


def _grammar_tree(root):
    if root.is_symlink() or not root.is_dir():
        raise ValueError('native isolation grammar is unavailable')
    paths = sorted(root.rglob('*'))
    if len(paths) > 4096:
        raise ValueError('native isolation grammar has too many paths')
    result = {}
    total = 0
    for path in paths:
        if path.is_symlink():
            raise ValueError('native isolation grammar contains a symlink')
        if path.is_file():
            if path.stat().st_size > 32 * 1024 * 1024:
                raise ValueError('native isolation grammar file exceeds size bound')
            with path.open('rb') as stream:
                data = stream.read(32 * 1024 * 1024 + 1)
            if len(data) > 32 * 1024 * 1024:
                raise ValueError('native isolation grammar file exceeds size bound')
            total += len(data)
            if total > 256 * 1024 * 1024:
                raise ValueError('native isolation grammar exceeds size bound')
            result[str(path.relative_to(root))] = hashlib.sha256(data).hexdigest()
    return result


def _control(campaign, control, read_proof):
    if not isinstance(control, dict):
        raise ValueError('native isolation control is missing')
    paths, decoded = {}, {}
    for name in ('result', 'identity', 'population'):
        path, data = read_proof(control.get(name))
        if not _allowed(campaign, path):
            raise ValueError('native isolation artifact escapes allowed scratch')
        paths[name], decoded[name] = path.resolve(), json.loads(data)
    root = paths['result'].parent
    if (paths['result'].name != 'result.json' or paths['identity'] != root/'identity.json'
            or paths['population'] != root/'checks/slice.json'):
        raise ValueError('native isolation control artifacts do not share a run')
    if not isinstance(decoded['identity'], dict) or decoded['identity'].get('tree') != _grammar_tree(root/'grammar'):
        raise ValueError('native isolation grammar hash differs')
    return decoded, control['population']['sha256']


def verify(campaign, job_id, attempt_id, checkpoint, rejection, evidence, read_proof, gates):
    if not isinstance(evidence, dict) or evidence.get('kind') != 'isolated-batch-native-revalidation':
        raise ValueError('isolated native batch revalidation evidence is required')
    paths, objects = {}, {}
    for name in ('manifest', 'validation', 'repository_log', 'isolation'):
        path, data = read_proof(evidence.get(name))
        paths[name] = path.resolve()
        objects[name] = data if name == 'repository_log' else json.loads(data)
    manifest, validation, isolation = (objects[name] for name in ('manifest', 'validation', 'isolation'))
    batch = manifest.get('id')
    if (not isinstance(batch, str) or not batch or Path(batch).name != batch
            or paths['manifest'] != (campaign/'.cache/batches'/batch/'manifest.json').resolve()
            or paths['validation'] != paths['manifest'].parent/'validation.json'
            or paths['repository_log'] != paths['manifest'].parent/'repository-check.log'
            or not _allowed(campaign, paths['isolation'])):
        raise ValueError('native batch evidence paths differ')
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]
    matches = [item for item in included if item.get('job_id') == job_id and item.get('attempt_id') == attempt_id]
    full = validation.get('full_checks', {})
    cp = json.loads((campaign/'checkpoints'/f'{checkpoint}.json').read_bytes())
    inputs = Path(cp['inputs']).resolve()
    if cp.get('commit') != checkpoint or not inputs.is_relative_to(campaign.resolve()):
        raise ValueError('native batch checkpoint inputs differ')
    frozen = json.loads((inputs/'manifest.json').read_bytes())
    if (not included or len(ids) != len(set(ids)) or len(matches) != 1
            or manifest.get('base_commit') != checkpoint or frozen.get('commit') != checkpoint
            or type(frozen.get('population')) is not int or frozen['population'] <= 0
            or rejection.get('evidence', {}).get('manifest') != batch
            or rejection.get('evidence', {}).get('validation') != validation
            or validation.get('scope') != 'combined-batch' or validation.get('status') != 'failed'
            or validation.get('reason') != 'full allowed-population checks failed'
            or type(validation.get('repository_check_returncode')) is not int
            or validation['repository_check_returncode'] != 0
            or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != checkpoint
            or len(validation.get('included_jobs', [])) != len(ids)
            or set(validation['included_jobs']) != set(ids)
            or type(full.get('faces')) is not int or full['faces'] != frozen.get('population')
            or not isinstance(full.get('failures'), list) or not full['failures']):
        raise ValueError('original full-population native-failing batch does not match')
    if any(status not in ('passed', 'reused') for status in gates(objects['repository_log']).values()):
        raise ValueError('repository gates did not all pass')
    excluded = isolation.get('excluded_jobs')
    if (isolation.get('kind') != 'isolated-native-regressions' or isolation.get('manifest') != batch
            or isolation.get('candidate') != manifest.get('commit') or isolation.get('parent') != checkpoint
            or not isinstance(excluded, list) or not excluded or any(not isinstance(name, str) for name in excluded)
            or len(excluded) != len(set(excluded)) or not set(excluded) <= set(ids) or job_id in excluded
            or not isinstance(isolation.get('reason'), str) or not isolation['reason'].strip()):
        raise ValueError('native isolation does not exclude the proven culprit set')
    controls = isolation.get('controls')
    if not isinstance(controls, dict):
        raise ValueError('native isolation controls are missing')
    expected = {'base': [], 'full': list(range(len(ids))),
                'remaining': [i for i, name in enumerate(ids) if name not in set(excluded)]}
    named_failures = {failure['face'] for failure in full['failures']
                      if isinstance(failure, dict) and isinstance(failure.get('face'), str)}
    base_tree = {name[len('grammar/'):]: digest for name, digest in frozen.get('files', {}).items()
                 if name.startswith('grammar/')}
    if not base_tree or not isinstance(manifest.get('grammar'), dict) or not manifest['grammar']:
        raise ValueError('base or candidate grammar identity is unavailable')
    hashes = set()
    count = isolation.get('population_count')
    if type(count) is not int or count <= 0 or count > full['faces'] or not named_failures:
        raise ValueError('native isolation population is not explicit')
    for name in ('base', 'full', 'remaining'):
        control, population_hash = _control(campaign, controls.get(name), read_proof)
        identity, result, population = control['identity'], control['result'], control['population']
        cards = population.get('cards') if isinstance(population, dict) else None
        if (not isinstance(identity, dict) or identity.get('indices') != expected[name]
                or not isinstance(identity.get('tree'), dict) or not identity['tree']
                or not isinstance(result, dict) or type(result.get('faces')) is not int or result.get('faces') != count
                or not isinstance(result.get('failures'), list)
                or (name == 'full') != bool(result['failures'])
                or not isinstance(cards, list) or any(not isinstance(card, str) for card in cards)
                or len(cards) != len(set(cards)) or len(cards) > count or not named_failures <= set(cards)):
            raise ValueError('native isolation control does not prove the specified population and outcome: '+name)
        if name == 'base' and identity['tree'] != base_tree:
            raise ValueError('native isolation base grammar differs')
        if name == 'full' and identity['tree'] != manifest['grammar']:
            raise ValueError('native isolation full grammar differs')
        hashes.add(population_hash)
    if len(hashes) != 1:
        raise ValueError('native isolation controls used different populations')
    return matches[0], {'manifest': batch, 'excluded_jobs': excluded, 'focused_faces': count,
                        'requires_fresh_full_validation': True}
