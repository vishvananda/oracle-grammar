"""Verify complete archived parse or print failures and replay focused survivors."""
from collections import Counter, defaultdict
from contextlib import closing
import fcntl
import hashlib
from itertools import zip_longest
import json
from pathlib import Path
import re
import sqlite3
import tempfile

import retire as r
import retire_library_readmission as library
import retire_parse_screen as screen
import retire_tasks as tasks
import retire_worker as worker
from retire_native_readmission import _allowed, _control, _grammar_tree

ARTIFACTS = ('commands.jsonl', 'gold.jsonl', 'oracle.json', 'slice.json', 'before.jsonl',
             'rows.jsonl', 'native.json', 'native-failures.jsonl', 'score.log', 'validate-readings.log')
PRINT_ONLY_REJECTION = 'full allowed-population checks failed'


def _lease_held(path):
    try:
        with path.open('rb') as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return True
    except FileNotFoundError:
        pass
    return False


def _focused_print_losses(failures):
    return (isinstance(failures, list) and 1 <= len(failures) <= 16
            and all(isinstance(row, dict) and set(row) == {'face', 'lost'}
                    and isinstance(row['face'], str) and row['face']
                    and row['lost'] == ['exact_default'] for row in failures))


def pending_print_batches(campaign, output, *, include_active=False):
    """Find intact current-attempt groups eligible for automatic recovery."""
    groups = defaultdict(set)
    uri = (campaign/'queue.sqlite').as_uri()+'?mode=ro'
    with closing(sqlite3.connect(uri, uri=True)) as db:
        for job, attempt, batch in db.execute("""
                SELECT id, attempt_id, json_extract(adjudication,'$.evidence.manifest')
                FROM jobs WHERE status='integration-rejected'
                AND json_extract(adjudication,'$.evidence.validation.reason')=?
                """, (PRINT_ONLY_REJECTION,)):
            if isinstance(batch, str) and re.fullmatch(r'[0-9a-f]{32}', batch):
                groups[batch].add((job, attempt))
    ready = []
    for batch, current in groups.items():
        if not 2 <= len(current) <= 96:
            continue
        if (output/batch).exists() and not (include_active and _lease_held(output/f'{batch}.lock')):
            continue
        folder = campaign/'.cache/batches'/batch
        try:
            manifest = r.read(folder/'manifest.json')
            validation = r.read(folder/'validation.json')
            included = [job for job in manifest['jobs'] if job['status'] == 'included']
            original = {(job['job_id'], job['attempt_id']) for job in included}
            if (manifest['id'] == batch and current == original and len(included) == len(original)
                    and validation['reason'] == PRINT_ONLY_REJECTION and validation['status'] == 'failed'
                    and validation['scope'] == 'combined-batch'
                    and _focused_print_losses(validation['full_checks']['failures'])):
                ready.append((folder.stat().st_mtime_ns, batch))
        except (OSError, KeyError, ValueError, TypeError):
            # Legacy or incomplete receipts remain available for review.
            continue
    return [batch for _, batch in sorted(ready)]


def awaiting_print_recovery(campaign):
    """Keep workers available while a live watcher can readmit a final cohort."""
    if not _lease_held(campaign/'full-print-recovery.lock'):
        return False
    marker = campaign/'full-print-recovery.json'
    try:
        output = Path(r.read(marker)['output'])
    except (OSError, KeyError, ValueError, TypeError):
        return False
    return bool(output.is_absolute() and _allowed(campaign, output)
                and pending_print_batches(campaign, output, include_active=True))


class Reads:
    def __init__(self, reader):
        self.reader, self.reads, self.files = reader, [], []

    def __call__(self, spec):
        path, data = self.reader(spec)
        self.reads.append((dict(spec), path.resolve(), hashlib.sha256(data).hexdigest()))
        return path, data

    def artifact(self, spec, expected):
        if (not isinstance(spec, dict) or set(spec) != {'path', 'sha256'}
                or spec.get('path') != str(expected) or not isinstance(spec.get('sha256'), str)
                or not re.fullmatch(r'[0-9a-f]{64}', spec['sha256']) or expected.is_symlink()
                or not expected.is_file() or expected.stat().st_size > 1024*1024*1024
                or r.digest(expected) != spec['sha256']):
            raise ValueError('parse isolation artifact path, size or hash differs')
        self.files.append((expected, spec['sha256']))

    def recheck(self):
        for spec, path, digest in self.reads:
            new_path, data = self.reader(spec)
            if new_path.resolve() != path or hashlib.sha256(data).hexdigest() != digest:
                raise ValueError('parse isolation proof changed during verification')
        if any(r.digest(path) != digest for path, digest in self.files):
            raise ValueError('parse isolation artifact changed during verification')


def _frozen(campaign, checkpoint, manifest, validation, reads, archived=None):
    cp_path = campaign/'checkpoints'/f'{checkpoint}.json'
    cp = r.read(cp_path); inputs = Path(cp['inputs']).resolve()
    run = (campaign/'.cache/batches'/manifest['id']/'validation-inputs').resolve()
    if (cp.get('commit') != checkpoint or not inputs.is_relative_to(campaign.resolve())
            or Path(validation.get('inputs', '')).resolve() != run):
        raise ValueError('parse readmission checkpoint identity differs')
    hashes = {key: validation.get(key) for key in ('checkpoint_manifest_sha256', 'validation_manifest_sha256')}
    if archived is not None:
        if not isinstance(archived, dict) or set(hashes) != set(archived):
            raise ValueError('archived checkpoint and candidate manifest pins are required')
        for key, root in zip(hashes, (inputs, run), strict=True):
            spec = archived[key]
            reads.artifact(spec, root/'manifest.json')
            if hashes[key] not in (None, spec['sha256']):
                raise ValueError('archive pin differs from the recorded validation manifest')
            hashes[key] = spec['sha256']
    parent, frozen = r.read(inputs/'manifest.json'), r.read(run/'manifest.json')
    trees = [{k.removeprefix('grammar/'):v for k,v in value.get('files', {}).items()
              if k.startswith('grammar/')} for value in (parent, frozen)]
    if (parent.get('commit') != checkpoint or frozen.get('commit') != manifest.get('commit')
            or type(parent.get('population')) is not int or parent['population'] <= 0
            or frozen.get('population') != parent['population'] or frozen.get('source') != parent.get('source')
            or r.digest(inputs/'manifest.json') != hashes['checkpoint_manifest_sha256']
            or r.digest(run/'manifest.json') != hashes['validation_manifest_sha256']
            or not trees[0] or trees[1] != manifest.get('grammar')
            or _grammar_tree(inputs/'frozen/grammar') != trees[0]
            or _grammar_tree(run/'frozen/grammar') != trees[1]):
        raise ValueError('parse frozen source or grammar identity differs')
    for name in ('allowed.json', 'gold.jsonl', 'oracle.json'):
        if not parent['files'].get(name) or frozen['files'].get(name) != parent['files'][name]:
            raise ValueError('parse frozen allowed population differs')
    for value, root in ((parent, inputs), (frozen, run)):
        build = r.read(root/'frozen/build.json')
        if (build.get('source_sha256') != value['source'] or not isinstance(build.get('compiler'), dict)
                or any(build.get('binaries', {}).get(name) != value['files'].get('bin/'+name) for name in r.HELPERS)):
            raise ValueError('parse frozen helper identity differs')
    if r.read(inputs/'frozen/build.json')['compiler'] != r.read(run/'frozen/build.json')['compiler']:
        raise ValueError('parse frozen compilers differ')
    r.verify(inputs); r.verify(run)
    data = worker.payload(run)
    baseline = Path(cp['baseline']).resolve()
    if not baseline.is_relative_to(campaign.resolve()) or r.digest(baseline) != validation.get('baseline_sha256'):
        raise ValueError('parse baseline hash differs')
    with baseline.open() as original, (run/'frozen/parse.jsonl').open() as saved:
        for old, parsed, entry in zip_longest(original, saved, data[0].values()):
            if (old is None or parsed is None or entry is None or json.loads(old) != json.loads(parsed)
                    or entry.get('parse') != json.loads(old) or entry.get('flags') != r.flags(json.loads(old))):
                raise ValueError('parse baseline population or print overlay differs')
    reads.files.extend((p, r.digest(p)) for p in (cp_path, inputs/'manifest.json', run/'manifest.json', baseline))
    return cp, inputs, run, data, trees


def _refusals(entries, rows, missing):
    refused, expected = [], []
    for (ordinal, entry), row in zip(entries, rows, strict=True):
        if row.get('name') != entry['name'] or row.get('text') != entry['text']:
            raise ValueError('parse refusal occurrence changed its name or wording')
        count = entry['parse']['readings']
        if row['status'] == 'refused':
            refused.append(ordinal)
            expected.extend((entry['name'], reading) for reading in range(count))
        elif row['status'] != 'equal' or row.get('readings') != count:
            raise ValueError('refusal-only isolation contains another reading change')
    if not refused or sorted(expected) != sorted((item['name'], item['reading']) for item in missing):
        raise ValueError('refusal-only isolation does not match the missing native readings')
    return refused


def _mixed_delta(entries, rows, preservation):
    """Identify lost and added readings by their original population ordinal."""
    missing, additions = preservation['missing'], preservation['additions']
    if not missing or not additions or preservation['merged_faces'] or preservation['passed']:
        raise ValueError('mixed parse isolation requires recorded reading losses and additions')
    names = Counter(entry['name'] for _, entry in entries)
    lost_names = {item['name'] for item in missing}
    by_name = {entry['name']: (ordinal, entry['parse']['readings']) for ordinal, entry in entries
               if entry['name'] in lost_names and names[entry['name']] == 1}
    losses = set()
    for item in missing:
        ordinal, old_count = by_name.get(item['name'], (None, 0))
        identity = (ordinal, item['reading'])
        if not 0 <= item['reading'] < old_count or identity in losses:
            raise ValueError('mixed parse isolation cannot identify the lost reading uniquely')
        losses.add(identity)
    loss_count = Counter(ordinal for ordinal, _ in losses)
    added, gain_count = [], {}
    for item in additions:
        local = item['occurrence']
        ordinal, _ = entries[local]
        gained = set(item['added_current_indices'])
        retained = set(item['retained_current_indices'])
        if (ordinal in gain_count or gained & retained
                or len(gained) + len(retained) != rows[local]['readings']):
            raise ValueError('mixed parse isolation has inconsistent added or retained indices')
        gain_count[ordinal] = len(gained)
        added.append(dict(item, occurrence=ordinal))
    for (ordinal, entry), row in zip(entries, rows, strict=True):
        current_count = entry['parse']['readings'] - loss_count[ordinal] + gain_count.get(ordinal, 0)
        status = 'equal' if current_count else 'refused'
        if (row['name'] != entry['name'] or row['text'] != entry['text']
                or row['status'] != status or row.get('readings', 0) != current_count):
            raise ValueError('mixed parse isolation has an unexplained row change')
    return added, sorted(losses)


def _original(run, folder, validation, data, reads):
    result = validation.get('parse_screen')
    if (not isinstance(result, dict) or result.get('status') != 'failed'
            or result.get('faces') != len(data[0]) or result.get('native_exit') not in (0, 1)
            or r.digest(folder/'parse-screen/result.json') != validation.get('parse_screen_result_sha256')
            or r.read(folder/'parse-screen/result.json') != result
            or not isinstance(result.get('artifacts'), dict) or set(result['artifacts']) != set(ARTIFACTS)):
        raise ValueError('original complete parse-screen result differs')
    for name in ARTIFACTS:
        reads.artifact(result['artifacts'][name], folder/'parse-screen'/name)
    recomputed = screen.assess(run, folder/'parse-screen', data, result['native_exit'])
    if recomputed != {k:v for k,v in result.items() if k != 'artifacts'}:
        raise ValueError('recomputed original parse-screen result differs')
    native = r.read(folder/'parse-screen/native.json')['reading_preservation']
    rows = r.rows(folder/'parse-screen/rows.jsonl')
    if native['missing'] and native['additions']:
        additions, losses = _mixed_delta(list(enumerate(data[0].values())), rows, native)
        return additions, [], losses
    if native['missing'] and native['merged_faces'] == 0 and not native['additions'] and not native['passed']:
        return [], _refusals(enumerate(data[0].values()), rows, native['missing']), []
    if (native['missing'] or native['merged_faces'] != 0 or not native['additions'] or not native['passed']
            or any(row.get('status') != 'equal' for row in rows)):
        raise ValueError('parse recovery requires a complete additions-only or refusal-only failure')
    return native['additions'], [], []


def _original_full_checks(run, folder, validation, data, reads, artifacts):
    checks = folder/'full-checks'
    if not isinstance(artifacts, dict) or set(artifacts) != set(ARTIFACTS):
        raise ValueError('archived full check artifacts are incomplete')
    for name in ARTIFACTS:
        reads.artifact(artifacts[name], checks/name)
    frozen = r.read(run/'manifest.json')['files']
    if any(artifacts[name]['sha256'] != frozen[name] for name in ('gold.jsonl', 'oracle.json')):
        raise ValueError('archived full check inputs differ from the frozen population')
    screen.assess(run, checks, data, 0)
    native = r.read(checks/'native.json')
    preserved = native['reading_preservation']
    rows = r.rows(checks/'rows.jsonl')
    count = len(data[0])
    selected = {entry['name'] for entry in data[0].values()}
    actual = {'faces': count, 'failures': worker.native_failures(native, count) + worker.compare(rows, data[0], selected)}
    named = {item['face'] for item in actual['failures'] if isinstance(item, dict)}
    if (actual != validation.get('full_checks') or not named or preserved['missing']
            or preserved['merged_faces'] != 0 or not preserved['passed']
            or any(row.get('status') != 'equal' for row in rows)):
        raise ValueError('archived full checks are not a complete addition or print failure')
    return preserved['additions'], actual['failures'], named


def _baseline_bridge(before_entries, current_entries):
    if len(before_entries) != len(current_entries):
        raise ValueError('parse control baseline occurrence identity differs')
    changed = []
    for old, (ordinal, current) in zip(before_entries, current_entries, strict=True):
        for entry in (old, current):
            if 'key' in entry['parse']:
                key = entry['parse']['key']
                if (not isinstance(key, list) or len(key) > 16384
                        or any(not isinstance(token, str) or not 1 <= len(token) <= 4096 for token in key)
                        or sum(len(token) for token in key) > 1024*1024):
                    raise ValueError('baseline derivation key has an invalid metadata shape')
        if (('key' in old['parse']) != ('key' in current['parse'])
                or {k:v for k,v in old.items() if k != 'parse'} != {k:v for k,v in current.items() if k != 'parse'}
                or {k:v for k,v in old['parse'].items() if k != 'key'} != {k:v for k,v in current['parse'].items() if k != 'key'}):
            raise ValueError('parse control baseline meanings, wording or print flags differ')
        if old['parse'].get('key') != current['parse'].get('key'):
            changed.append({'occurrence': ordinal, 'face': current['name'],
                            'control_key': old['parse']['key'], 'screen_key': current['parse']['key']})
    return changed


def _focus(campaign, controls, profile_spec, context, reads):
    data, run, expected, included, trees, original, count = (context[k] for k in
        ('data', 'run', 'expected', 'included', 'trees', 'original', 'count'))
    path, payload = reads(profile_spec)
    if not _allowed(campaign, path): raise ValueError('parse focused profile escapes scratch')
    profile = json.loads(payload)
    names = profile.get('cards') if isinstance(profile, dict) and set(profile) == {'cards'} else None
    full_failures = context.get('full_failures')
    named = context['named_failures']
    if (not isinstance(names, list) or not 1 <= len(names) <= 500 or len(set(names)) != len(names)
            or any(not isinstance(name, str) or not name for name in names) or not named <= set(names)):
        raise ValueError('parse focused profile omits a failure or has duplicate/invalid names')
    if full_failures is not None and set(names) != named:
        raise ValueError('archived full checks require the exact named failure profile')
    closure = tasks.close_cohort(run, {'names': names}, max_occurrences=1024)
    if closure['closure_occurrence_count'] != count:
        raise ValueError('parse focused population count differs from complete closure')
    selected = set(closure['names'])
    entries = [(ordinal, entry) for ordinal, entry in enumerate(data[0].values()) if entry['name'] in selected]
    control_data = context['control_data']
    before_entries = [entry for entry in control_data[0].values() if entry['name'] in selected]
    key_changes = _baseline_bridge(before_entries, entries)
    if context['has_declared_key_changes'] and context['declared_key_changes'] != key_changes:
        raise ValueError('declared derivation-key bridge differs from immutable baselines')
    context['derivation_key_changes'] = key_changes
    focused = ({str(i):entry for i, entry in enumerate(before_entries)}, control_data[1], control_data[2])
    with tempfile.TemporaryDirectory(prefix='.parse-focus-') as temporary:
        projection = Path(temporary); (projection/'frozen').mkdir()
        # Only an assessor projection: original immutable input identity is verified separately.
        r.write(projection/'manifest.json', {'population': count})
        r.write(projection/'frozen/allowed.json', {'cards': sorted(selected)})
        worker.subset(projection, selected, control_data)
        for label in ('base', 'full', 'remaining'):
            decoded, _ = _control(campaign, controls.get(label), reads)
            identity, result, population = (decoded[k] for k in ('identity', 'result', 'population'))
            root = Path(controls[label]['identity']['path']).resolve().parent; checks = root/'checks'
            if (identity.get('method') != 'ordered-proposal-replay-v1' or identity.get('indices') != expected[label]
                    or identity.get('proposals') != [{'job_id': included[i]['job_id'], 'attempt_id': included[i]['attempt_id']}
                                                     for i in expected[label]]
                    or population != {'cards': sorted(selected)}):
                raise ValueError('parse focused ordered identity or exact population differs')
            if label in ('base', 'full') and identity['tree'] != trees[label == 'full']:
                raise ValueError('parse focused base or full grammar differs')
            artifacts = controls[label].get('artifacts')
            if not isinstance(artifacts, dict) or set(artifacts) != set(ARTIFACTS):
                raise ValueError('parse focused control artifacts are incomplete')
            for name in ARTIFACTS:
                reads.artifact(artifacts[name], checks/name)
            for name in ('gold.jsonl', 'oracle.json', 'slice.json', 'before.jsonl'):
                if (checks/name).read_bytes() != (projection/name).read_bytes():
                    raise ValueError('parse focused input or baseline differs from frozen subset')
            commands = r.rows(checks/'commands.jsonl')
            if len(commands) != 2 or commands[1].get('returncode') not in (0, 1):
                raise ValueError('parse focused native command is incomplete')
            screen.assess(projection, checks, focused, commands[1]['returncode'])
            native = r.read(checks/'native.json')
            rows = r.rows(checks/'rows.jsonl')
            actual = {'faces': count, 'failures': worker.native_failures(native, count)
                      + worker.compare(rows, data[0], selected)}
            if result != actual or (label == 'full') != bool(actual['failures']):
                raise ValueError('parse focused native or print outcome differs')
            if label == 'full' and full_failures is not None and actual['failures'] != full_failures:
                raise ValueError('focused full checks did not reproduce the exact archived failures')
            preservation = native['reading_preservation']
            if label == 'full' and context['refusals']:
                if (preservation['additions'] or preservation['merged_faces'] or preservation['passed']
                        or _refusals(entries, rows, preservation['missing']) != context['refusals']):
                    raise ValueError('focused full checks did not reproduce the same refused occurrences')
                continue
            if label == 'full' and context['losses']:
                if _mixed_delta(entries, rows, preservation) != (original, context['losses']):
                    raise ValueError('focused full checks did not reproduce the same reading losses and additions')
                continue
            additions = [dict(item, occurrence=entries[item['occurrence']][0]) for item in preservation['additions']]
            if preservation['missing'] or preservation['merged_faces'] or (label == 'full' and additions != original):
                raise ValueError('parse full focus does not exactly reproduce original added readings')
    return {label: _control(campaign, controls[label], reads)[0]['identity']['tree'] for label in controls}


def verify(campaign, selected, checkpoint, rejections, evidence, read_proof, gates):
    reads = Reads(read_proof); paths, values = {}, {}
    for name in ('manifest', 'validation', 'repository_log', 'isolation'):
        path, value = reads(evidence.get(name)); paths[name] = path.resolve()
        values[name] = value if name == 'repository_log' else json.loads(value)
    manifest, validation, isolation = (values[k] for k in ('manifest', 'validation', 'isolation'))
    kind = evidence.get('kind')
    full_checks = kind == 'isolated-full-check-revalidation'
    batch = manifest.get('id'); folder = campaign/'.cache/batches'/str(batch)
    included = [item for item in manifest.get('jobs', []) if item.get('status') == 'included']
    ids = [item.get('job_id') for item in included]; excluded = isolation.get('excluded_jobs')
    if (kind not in ('isolated-parse-screen-revalidation', 'isolated-full-check-revalidation')
            or not isinstance(batch, str)
            or not batch or Path(batch).name != batch or not included or len(included) > 96
            or any(not isinstance(name, str) or not name for name in ids) or len(set(ids)) != len(ids)
            or not isinstance(excluded, list) or not excluded or len(set(excluded)) != len(excluded)
            or any(not isinstance(name, str) for name in excluded) or not set(excluded) <= set(ids)
            or set(selected) != set(ids)-set(excluded)
            or any(item.get('attempt_id') != selected[item['job_id']] for item in included if item['job_id'] in selected)
            or paths['manifest'] != (folder/'manifest.json').resolve()
            or paths['validation'] != (folder/'validation.json').resolve()
            or paths['repository_log'] != (folder/'repository-check.log').resolve() or not _allowed(campaign, paths['isolation'])):
        raise ValueError('parse isolation selection or original artifact paths differ')
    reason = 'full allowed-population checks failed' if full_checks else 'full allowed-population parse screen failed'
    if (manifest.get('base_commit') != checkpoint or validation.get('scope') != 'combined-batch'
            or validation.get('status') != 'failed' or validation.get('reason') != reason
            or validation.get('repository_check_returncode') != 0 or validation.get('candidate') != manifest.get('commit')
            or validation.get('source_checkpoint') != checkpoint or validation.get('included_jobs') != ids
            or ('full_checks' in validation) != full_checks or 'rows' in validation
            or any(x.get('evidence', {}).get('manifest') != batch or x.get('evidence', {}).get('validation') != validation
                   for x in rejections.values())
            or any(value not in ('passed', 'reused') for value in gates(values['repository_log']).values())):
        outcome = 'archived full-check' if full_checks else 'original additions-only parse'
        raise ValueError(outcome+' failure or complete repository gates differ')
    isolation_kind = 'isolated-full-check-regressions' if full_checks else 'isolated-parse-screen-regressions'
    if (isolation.get('kind') != isolation_kind or isolation.get('manifest') != batch
            or isolation.get('candidate') != manifest.get('commit') or isolation.get('parent') != checkpoint
            or not isinstance(isolation.get('reason'), str) or not isolation['reason'].strip()
            or not isinstance(isolation.get('controls'), dict) or set(isolation['controls']) != {'base', 'full', 'remaining'}):
        raise ValueError('parse isolation identity differs')
    archive = evidence.get('archive_manifests', {}) if full_checks else None
    cp, inputs, run, data, trees = _frozen(campaign, checkpoint, manifest, validation, reads, archive)
    refused, losses = [], []
    if full_checks:
        original, full_failures, named = _original_full_checks(
            run, folder, validation, data, reads, evidence.get('full_artifacts'))
    else:
        original, refused, losses = _original(run, folder, validation, data, reads)
        named = {item['name'] for item in original}
        if refused:
            entries = list(data[0].values())
            named = {entries[ordinal]['name'] for ordinal in refused}
        if losses:
            entries = list(data[0].values())
            named.update(entries[ordinal]['name'] for ordinal, _ in losses)
        full_failures = None
    expected = {'base': [], 'full': list(range(len(ids))),
                'remaining': [i for i, job in enumerate(ids) if job not in excluded]}
    count = isolation.get('population_count')
    if type(count) is not int or not 1 <= count <= min(1024, len(data[0])):
        raise ValueError('parse isolation population is invalid')
    # Full-check controls use the accepted audit rows frozen in validation inputs.
    # The accepted checkpoint's input index can retain older print flags.
    control_data = data if full_checks else worker.payload(inputs)
    focus = {'data': data, 'control_data': control_data, 'run': run, 'expected': expected, 'included': included,
             'trees': trees, 'original': original, 'count': count, 'named_failures': named,
             'full_failures': full_failures, 'refusals': refused, 'losses': losses,
             'has_declared_key_changes': 'derivation_key_changes' in isolation,
             'declared_key_changes': isolation.get('derivation_key_changes')}
    control_trees = _focus(campaign, isolation['controls'], isolation.get('profile'), focus, reads)
    reports_path, reports_data = reads(isolation.get('original_reports'))
    reports = json.loads(reports_data)
    if not _allowed(campaign, reports_path) or not isinstance(reports, dict) or set(reports) != set(ids):
        raise ValueError('parse replay reports do not cover all original included attempts')
    if any(reports.get(job) != evidence['reports'].get(job) for job in selected):
        raise ValueError('parse selected original report identities differ')
    for spec in reports.values(): reads(spec)
    for label in ('full', 'remaining'):
        library._replay(campaign, checkpoint, included, expected[label], control_trees[label],
                        paths['isolation'].parent, reports, canonical_mechanical=True)
    reads.recheck(); r.verify(inputs); r.verify(run)
    if (_grammar_tree(inputs/'frozen/grammar') != trees[0] or _grammar_tree(run/'frozen/grammar') != trees[1]
            or any(_grammar_tree(Path(isolation['controls'][label]['identity']['path']).parent/'grammar') != tree
                   for label, tree in control_trees.items())):
        raise ValueError('parse frozen or focused grammar changed during replay')
    artifacts = {str(path): digest for path, digest in reads.files}
    artifacts.update({str(path): digest for _, path, digest in reads.reads})
    grammars = {str(inputs/'frozen/grammar'): trees[0], str(run/'frozen/grammar'): trees[1]}
    grammars.update({str(Path(isolation['controls'][label]['identity']['path']).parent/'grammar'): tree
                     for label, tree in control_trees.items()})
    proof_scope = 'archived full-check isolation' if full_checks else 'additions-only parse-screen isolation'
    if refused:
        proof_scope = 'refusal-only parse-screen isolation'
    if losses:
        proof_scope = 'mixed reading losses and additions in parse-screen isolation'
    return {'manifest': batch, 'excluded_jobs': excluded, 'focused_faces': count,
            'requires_fresh_full_validation': True,
            'proof_scope': proof_scope,
            'verified_artifacts': artifacts, 'verified_grammars': grammars,
            'verified_frozen_inputs': [str(inputs), str(run)],
            'derivation_key_changes': focus['derivation_key_changes']}


def recheck(proof):
    for path in proof['verified_frozen_inputs']:
        r.verify(Path(path))
    if (any(r.digest(Path(path)) != digest for path, digest in proof['verified_artifacts'].items())
            or any(_grammar_tree(Path(path)) != tree for path, tree in proof['verified_grammars'].items())):
        raise ValueError('parse readmission artifacts changed before transaction')
