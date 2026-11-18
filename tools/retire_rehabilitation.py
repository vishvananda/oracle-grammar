"""Verify accepted checkpoint scan inputs for parked-rule rehabilitation."""
import hashlib
import json
from pathlib import Path
import re

MAX_BYTES = 64 * 1024 * 1024


def _address(path):
    return {'path': str(path.resolve()), 'sha256': _digest_large(path)}


def scan_evidence(inputs):
    return {'kind': 'checkpoint-unused-rehabilitation',
            'manifest': _address(inputs/'manifest.json'), 'scan': _address(inputs/'scan/rules.json'),
            'scan_complete': _address(inputs/'scan-complete.json'), 'commands': _address(inputs/'scan/commands.jsonl')}


def mechanical_print_escalation(inputs, report):
    """Collect artifacts for an independently verified mechanical escalation."""
    return {'kind': 'mechanical-author-escalation', 'scan': scan_evidence(inputs),
            'index': _address(inputs/'frozen/index.json'), 'report': _address(report),
            'artifacts': {name: _address(report.parent/name) for name in MECHANICAL_OUTCOME_FILES}}


def _read(path, digest=None):
    if path.is_symlink():
        raise ValueError('rehabilitation input must not be a symlink')
    with path.open('rb') as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES:
        raise ValueError('rehabilitation input exceeds size bound')
    actual = hashlib.sha256(data).hexdigest()
    if digest is not None and digest != actual:
        raise ValueError('rehabilitation input hash differs')
    return data, actual


def _artifact(evidence, key, expected):
    spec = evidence.get(key)
    if not isinstance(spec, dict) or not isinstance(spec.get('path'), str) or not isinstance(spec.get('sha256'), str):
        raise ValueError('hashed rehabilitation artifact required: '+key)
    path = Path(spec['path'])
    if not path.is_absolute() or path.is_symlink() or path.resolve() != expected.resolve():
        raise ValueError('rehabilitation artifact path differs: '+key)
    return _read(path, spec['sha256'])


def verify_scan(campaign, checkpoint, checkpoint_evidence, evidence):
    """Verify rule-audit completion and bind scan to this accepted checkpoint."""
    if not isinstance(evidence, dict) or evidence.get('kind') != 'checkpoint-unused-rehabilitation':
        raise ValueError('checkpoint-unused-rehabilitation evidence is required')
    if not isinstance(checkpoint, str) or not checkpoint or Path(checkpoint).name != checkpoint:
        raise ValueError('invalid checkpoint identity')
    record = json.loads(_read(campaign/'checkpoints'/f'{checkpoint}.json')[0])
    if record.get('commit') != checkpoint or not isinstance(record.get('inputs'), str):
        raise ValueError('checkpoint record differs')
    inputs = Path(record['inputs']).resolve()
    if not inputs.is_relative_to(campaign.resolve()):
        raise ValueError('checkpoint inputs escape campaign')
    accepted = checkpoint_evidence if isinstance(checkpoint_evidence, dict) else {}
    validation = accepted.get('validation') if isinstance(accepted.get('validation'), dict) else {}
    accepted_inputs = accepted.get('inputs') or validation.get('inputs')
    if accepted_inputs and Path(accepted_inputs).resolve() != inputs:
        raise ValueError('checkpoint evidence names different inputs')
    if not accepted_inputs and accepted.get('kind') != 'accepted-input':
        raise ValueError('accepted checkpoint evidence does not bind inputs')
    manifest_data, manifest_hash = _artifact(evidence, 'manifest', inputs/'manifest.json')
    manifest = json.loads(manifest_data)
    if (manifest.get('format') != 3 or manifest.get('commit') != checkpoint
            or type(manifest.get('population')) is not int or manifest['population'] <= 0):
        raise ValueError('frozen manifest does not match checkpoint')
    if accepted.get('kind') == 'accepted-input' and accepted.get('manifest') != manifest_hash:
        raise ValueError('initial checkpoint manifest differs')
    scan_data, scan_hash = _artifact(evidence, 'scan', inputs/'scan/rules.json')
    complete = json.loads(_artifact(evidence, 'scan_complete', inputs/'scan-complete.json')[0])
    commands = [json.loads(line) for line in _artifact(evidence, 'commands', inputs/'scan/commands.jsonl')[0].splitlines() if line.strip()]
    audits = [entry for entry in commands if entry.get('program') == 'rule-audit']
    if (complete.get('report') != scan_hash or len(audits) != 1
            or type(audits[0].get('returncode')) is not int or audits[0]['returncode'] != 0):
        raise ValueError('rule-audit completion does not match scan')
    report = json.loads(scan_data)
    summary, unused = report.get('summary'), report.get('unused')
    if (not isinstance(summary, dict)
            or any(type(summary.get(key)) is not int for key in ('faces', 'equal', 'at_risk', 'unused_rules'))
            or summary.get('faces') != manifest['population']
            or summary.get('equal') != manifest['population'] or summary.get('at_risk') != 0
            or not isinstance(unused, list) or len(unused) > 200_000
            or any(not isinstance(rule, str) or not re.fullmatch(r'[mt]_\w+', rule) for rule in unused)
            or len(unused) != len(set(unused)) or summary.get('unused_rules') != len(unused)):
        raise ValueError('scan is incomplete or has invalid unused rules')
    return inputs, set(unused), manifest_hash, scan_hash


def verify_rule_receipts(inputs, rules, manifest_hash, scan_hash):
    """Confirm each unused rule was recorded against this scan and manifest."""
    for rule in rules:
        name = hashlib.sha256(rule.encode()).hexdigest()[:20]
        receipt = json.loads(_read(inputs/'jobs'/name/'job.json')[0])
        if receipt != {'rule': rule, 'scan': scan_hash, 'snapshot': manifest_hash}:
            raise ValueError('unused rule scan receipt differs')


def resolve_origin(db, job_id, row):
    """Find the original immutable terminal attempt for a parked job."""
    status, attempt_id, payload_raw, adjudication = row
    payload = json.loads(payload_raw)
    # A real new attempt supersedes any inherited review for current decisions.
    origin = payload.get('review_origin') if attempt_id is None else None
    if origin is None:
        if not attempt_id:
            return None
        origin_job, origin_attempt = job_id, attempt_id
        original_status = status
        original_adjudication = adjudication
    else:
        if (not isinstance(origin, dict) or attempt_id is not None
                or not isinstance(origin.get('job_id'), str) or not isinstance(origin.get('attempt_id'), str)
                or origin.get('status') not in ('needs-review', 'integration-rejected', 'failed')):
            raise ValueError('invalid inherited review origin')
        origin_job, origin_attempt = origin['job_id'], origin['attempt_id']
        previous = db.execute('SELECT status,attempt_id,payload,adjudication FROM jobs WHERE id=?', (origin_job,)).fetchone()
        if (not previous or previous[0] != 'superseded' or previous[1] != origin_attempt
                or not set(payload['rules']) <= set(json.loads(previous[2])['rules'])):
            raise ValueError('inherited review origin no longer matches original job')
        linked = db.execute(
            "SELECT 1 FROM events WHERE json_extract(data,'$.origin_attempt')=? AND "
            "((kind IN ('rehabilitated-unused','split-reviewed-author') AND json_extract(data,'$.review_job')=?) "
            "OR (kind='split-reviewed-author' AND json_extract(data,'$.job_id')=?))",
            (origin_attempt, job_id, job_id)).fetchone()
        if not linked:
            raise ValueError('inherited review origin has no split event')
        original_status, original_adjudication = origin['status'], previous[3]
    attempt = db.execute('SELECT job_id,base_commit,started,result FROM attempts WHERE id=?', (origin_attempt,)).fetchone()
    if not attempt or attempt[0] != origin_job or not attempt[3]:
        raise ValueError('original terminal attempt is missing')
    result_hash = hashlib.sha256(attempt[3].encode()).hexdigest()
    adjudication_hash = hashlib.sha256((original_adjudication if original_adjudication is not None else 'null').encode()).hexdigest()
    resolved = {'job_id': origin_job, 'attempt_id': origin_attempt, 'result_sha256': result_hash,
                'adjudication_sha256': adjudication_hash, 'status': original_status}
    if origin is not None and origin != resolved:
        raise ValueError('inherited review origin hash differs')
    result = json.loads(attempt[3])
    expected_status = {'needs-review': 'needs-review', 'integration-rejected': 'provisional-pass', 'failed': 'failed'}
    if result.get('status') != expected_status[original_status]:
        raise ValueError('original attempt status differs from parked disposition')
    if not isinstance(result.get('outcomes'), dict) or not set(payload['rules']) <= result['outcomes'].keys():
        raise ValueError('original outcomes do not cover parked rules')
    if origin is None and set(payload['rules']) != set(result['outcomes']):
        raise ValueError('original outcomes differ from assigned rules')
    return payload, resolved, attempt[1], attempt[2], attempt[3]


def _digest_large(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


MECHANICAL_OUTCOME_FILES = ('commands.jsonl', 'native.json', 'native-failures.jsonl',
                            'before.jsonl', 'rows.jsonl', 'slice.json')


def recheck_mechanical_failure(inputs, evidence):
    report = Path(evidence['report']['path'])
    artifacts = evidence.get('artifacts')
    if not isinstance(artifacts, dict) or set(artifacts) != set(MECHANICAL_OUTCOME_FILES):
        raise ValueError('complete mechanical outcome artifacts are required')
    data = {name: _artifact(artifacts, name, report.parent/name)[0] for name in MECHANICAL_OUTCOME_FILES}
    # The full frozen index can be larger than a focused diagnostic artifact.
    index = evidence.get('index', {})
    path = inputs/'frozen/index.json'
    if (index.get('path') != str(path) or path.is_symlink() or path.stat().st_size > 1024*1024*1024
            or _digest_large(path) != index.get('sha256')):
        raise ValueError('mechanical diagnostic index differs')
    _artifact(evidence['scan'], 'manifest', inputs/'manifest.json')
    return data


def mechanical_failure(inputs, run, rules, report, evidence):
    """Recompute the named semantic regressions of a verified pure removal."""
    import subprocess
    import tempfile

    import retire as r
    import retire_batch as batch
    import retire_native_outcome as native_outcome
    import retire_parse_screen as screen
    import retire_worker as worker

    directory = Path(report['directory'])
    task = r.read(run/'tasks'/f"{report['task']}.json")
    if ((run/'inputs').resolve() != inputs or task.get('rules') != rules or task.get('kind') != 'mechanical'
            or task.get('mechanical_chunks') is not None or 'chunks_expected' in report):
        raise ValueError('unpartitioned mechanical report differs from its assigned inputs')
    manifest = r.read(inputs/'manifest.json')
    if evidence['index'].get('sha256') != manifest['files'].get('index.json'):
        raise ValueError('mechanical diagnostic index differs from the frozen manifest')
    frozen = r.verify(inputs)
    with tempfile.TemporaryDirectory(prefix='retire-mechanical-proof-') as scratch:
        root = Path(scratch)
        batch._mechanical_removal(run, directory, rules, root)
        raw = recheck_mechanical_failure(inputs, evidence)
        names = set(json.loads(raw['slice.json'])['cards'])
        if not 1 <= len(names) <= 1024 or names != set(task['cohort']):
            raise ValueError('mechanical diagnostic cohort differs')
        index = {key: entry for key,entry in r.read(inputs/'frozen/index.json').items() if entry['name'] in names}
        (root/'frozen').mkdir()
        r.write(root/'manifest.json', {'population': len(index)})
        r.write(root/'frozen/allowed.json', {'cards': sorted(names)})
        for name, value in raw.items():
            (root/name).write_bytes(value)
        commands = [json.loads(line) for line in raw['commands.jsonl'].splitlines()]
        native = json.loads(raw['native.json'])
        completed = [(c.get('program'), c.get('returncode')) for c in commands]
        data = (index, None, None)
        if completed == [('score', 0), ('validate-readings', 1)]:
            command = [str((inputs/'frozen/bin/validate-readings').resolve()), str(directory/'gold.jsonl'),
                       str(directory/'rows.jsonl'), str(directory/'native.json'), str(directory/'native-failures.jsonl'),
                       '--preserve-from', str(directory/'before.jsonl')]
            error = subprocess.CalledProcessError(1, command)
            if report.get('failures') != [str(error)]:
                raise ValueError('mechanical native failure does not match its command')
            actual = native_outcome.preservation_failure(error, inputs, directory, data)
        else:
            screen.assess(root, root, data, 0)
            failures = worker.native_failures(native, len(index)) + worker.compare(r.rows(root/'rows.jsonl'), index, names)
            actual = {'faces': len(index), 'failures': failures}
            if actual != {key: report.get(key) for key in ('faces', 'failures')}:
                raise ValueError('mechanical native and print failure differs from its report')
        preserved = native['reading_preservation']
        faces = {item['face'] for item in actual['failures'] if isinstance(item, dict)}
        faces.update(item['name'] for key in ('missing', 'additions') for item in preserved[key])
        if not 1 <= len(faces) <= 500 or not faces <= names or sum(entry['name'] in faces for entry in index.values()) == 0:
            raise ValueError('mechanical failure lacks named target evidence')
    if r.verify(inputs) != frozen:
        raise ValueError('mechanical frozen inputs changed during review')
    return sorted(faces)
