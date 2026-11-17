"""Bounded evidence for splitting a finished model-timeout bundle."""
import hashlib
import json
from contextlib import closing
from dataclasses import dataclass
from pathlib import Path
import tarfile

import retire_rehabilitation

MAX_PROOF_BYTES = 32 * 1024 * 1024


@dataclass(frozen=True)
class TimeoutState:
    base: str
    payload: dict
    started_raw: str
    result_raw: str


def artifact(evidence, name, expected):
    spec = evidence.get(name)
    if not isinstance(spec, dict) or not isinstance(spec.get('path'), str) or not isinstance(spec.get('sha256'), str):
        raise ValueError('hashed timeout artifact required: '+name)
    path = Path(spec['path'])
    if not path.is_absolute() or path.is_symlink() or path.resolve() != expected.resolve():
        raise ValueError('timeout artifact path differs: '+name)
    with path.open('rb') as stream:
        data = stream.read(MAX_PROOF_BYTES+1)
    if len(data) > MAX_PROOF_BYTES or hashlib.sha256(data).hexdigest() != spec['sha256']:
        raise ValueError('timeout artifact hash or size differs: '+name)
    return data


def verify(campaign, job_id, attempt, state, evidence):
    """Check only the assigned timeout artifacts; never infer semantic success."""
    if not isinstance(evidence, dict) or evidence.get('kind') != 'timeout-split-recovery':
        raise ValueError('timeout-split-recovery evidence required')
    base, payload, started_raw, result_raw = state.base, state.payload, state.started_raw, state.result_raw
    run = campaign/'.cache/attempts'/attempt
    worker = run/'workers'/job_id
    started = json.loads(started_raw) if started_raw else {}
    result = json.loads(result_raw) if result_raw else {}
    terminal_path = campaign/'.cache/receipts'/f'{attempt}.json'
    raw = {name: artifact(evidence, name, path) for name, path in {
        'final': worker/'final.txt', 'model_receipt': worker/'receipt.json',
        'events': worker/'events.jsonl', 'descriptor': run/'tasks'/f'{job_id}.json',
        'support': run/'support.json', 'base_manifest': run/'inputs/manifest.json',
        'archive_metadata': run/'archive.json', 'archive': campaign/'.cache/archives'/f'{attempt}.tar.gz',
        'terminal_receipt': terminal_path}.items()}
    if raw['final'] != b'':
        raise ValueError('timeout final must be exactly empty')
    receipt, descriptor, support, manifest, metadata, terminal = (
        json.loads(raw[name]) for name in ('model_receipt', 'descriptor', 'support', 'base_manifest',
                                           'archive_metadata', 'terminal_receipt'))
    if (payload.get('kind') != 'author' or payload.get('timeout_origin') is not None
            or not isinstance(payload.get('rules'), list) or not 4 <= len(payload['rules']) <= 6
            or len(set(payload['rules'])) != len(payload['rules']) or descriptor.get('id') != job_id
            or descriptor.get('rules') != payload['rules']):
        raise ValueError('timeout recovery requires original four-to-six-rule author job')
    pid = started.get('pid')
    if (type(pid) is not int or pid <= 0 or receipt.get('pid') != pid
            or started.get('receipt') != str(worker/'receipt.json') or receipt.get('task') != job_id
            or receipt.get('timed_out') is not True or receipt.get('status') != 'needs-review'
            or type(receipt.get('returncode')) is not int or receipt['returncode'] == 0
            or type(descriptor.get('wall_budget_seconds')) is not int or descriptor['wall_budget_seconds'] < 900
            or type(receipt.get('started_unix')) not in (int, float)
            or type(receipt.get('finished_unix')) not in (int, float)
            or receipt['finished_unix']-receipt['started_unix'] < descriptor['wall_budget_seconds']):
        raise ValueError('model receipt does not prove the assigned timed-out process')
    events = [json.loads(line) for line in raw['events'].splitlines() if line.strip()]
    if (sum(event.get('type') == 'thread.started' for event in events) != 1
            or any(event.get('type') in ('turn.completed', 'turn.failed') for event in events)):
        raise ValueError('model events do not prove an unfinished assigned thread')
    dhash = hashlib.sha256(raw['descriptor']).hexdigest()
    if (support.get('tasks', {}).get(job_id) != dhash or manifest.get('commit') != base
            or manifest.get('format') != 3 or terminal.get('run') != str(run)
            or terminal.get('task') != job_id or terminal.get('base_commit') != base
            or terminal.get('status') != 'operational-failure'
            or terminal.get('error') != 'Expecting value: line 1 column 1 (char 0)'
            or terminal.get('dispositions') is not None
            or result.get('status') != 'needs-review'
            or result.get('evidence', {}).get('receipt') != str(terminal_path)
            or result.get('evidence', {}).get('dispositions') is not None
            or set(result.get('outcomes', {})) != set(payload['rules'])
            or any(v.get('status') != 'needs-review' for v in result['outcomes'].values())):
        raise ValueError('terminal outcome, descriptor or checkpoint identity differs')
    checkpoint = campaign/'checkpoints'/f'{base}.json'
    base_record = json.loads(checkpoint.read_text())
    if (base_record.get('commit') != base
            or (run/'inputs').resolve() != Path(base_record['inputs']).resolve()):
        raise ValueError('assigned frozen input differs from its base checkpoint')
    archive = campaign/'.cache/archives'/f'{attempt}.tar.gz'
    if (metadata.get('archive') != str(archive) or metadata.get('sha256') != hashlib.sha256(raw['archive']).hexdigest()
            or metadata.get('checkout_removed') is not True
            or descriptor.get('checkout') != str(campaign/'workspaces'/attempt)
            or (campaign/'workspaces'/attempt).exists() or (campaign/'workspaces'/attempt).is_symlink()):
        raise ValueError('verified archive or removed checkout differs')
    with tarfile.open(archive, 'r:gz') as tar:
        member = tar.getmember('manifest.json')
        if not member.isfile() or member.size > 1024*1024:
            raise ValueError('invalid timeout archive manifest')
        archived = json.load(tar.extractfile(member))
    if (archived.get('task') != job_id or archived.get('attempt') != attempt
            or archived.get('base_commit') != base or archived.get('descriptor_sha256') != dhash
            or archived.get('receipt_sha256') != hashlib.sha256(raw['terminal_receipt']).hexdigest()):
        raise ValueError('timeout archive does not bind the assigned attempt')
    return {'pid': pid, 'artifact_sha256': {name: hashlib.sha256(value).hexdigest() for name, value in raw.items()}}


def rehabilitate(queue, job_id, attempt, checkpoint, evidence):
    """Split one proved timeout, preserving its immutable attempt and archive."""
    if not isinstance(attempt, str) or not attempt or Path(attempt).name != attempt:
        raise ValueError('invalid timeout attempt identity')
    campaign = Path(queue.path).parent
    query = ('SELECT j.status,j.attempt_id,j.payload,j.adjudication,a.base_commit,a.started,a.result '
             'FROM jobs j JOIN attempts a ON a.id=j.attempt_id WHERE j.id=?')
    with queue._retry_lease(job_id):
        with closing(queue._connect()) as db:
            row = db.execute(query, (job_id,)).fetchone()
            latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
            if not row or row[0] != 'needs-review' or row[1] != attempt or not row[6]:
                raise ValueError('timeout split requires the current finished needs-review attempt')
            if not latest or latest[0] != checkpoint:
                raise ValueError('timeout split checkpoint is stale')
            snapshot, cp_snapshot = tuple(row), tuple(latest)
            base_row = db.execute('SELECT evidence FROM checkpoints WHERE commit_id=?', (row[4],)).fetchone()
            if not base_row:
                raise ValueError('timeout base checkpoint is not accepted')
            base_evidence_raw = base_row[0]
        payload = json.loads(snapshot[2])
        state = TimeoutState(snapshot[4], payload, snapshot[5], snapshot[6])
        proof = verify(campaign, job_id, attempt, state, evidence)
        accepted = json.loads(base_evidence_raw)
        if not isinstance(accepted, dict):
            raise ValueError('timeout base checkpoint evidence is invalid')
        validation = accepted.get('validation') if isinstance(accepted.get('validation'), dict) else {}
        expected_inputs = accepted.get('inputs') or validation.get('inputs')
        if accepted.get('kind') == 'accepted-input':
            if accepted.get('manifest') != proof['artifact_sha256']['base_manifest']:
                raise ValueError('timeout base manifest differs from accepted checkpoint')
        elif not expected_inputs or Path(expected_inputs).resolve() != (campaign/'.cache/attempts'/attempt/'inputs').resolve():
            raise ValueError('timeout base inputs differ from accepted checkpoint')
        queue._require_absent_pid(proof['pid'])
        scan_proof = evidence.get('scan_proof')
        scan = retire_rehabilitation.verify_scan(campaign, checkpoint, json.loads(cp_snapshot[1]), scan_proof)
        report = json.loads(retire_rehabilitation._read(scan[0]/'scan/rules.json', scan[3])[0])
        needed = report.get('needed')
        if not isinstance(needed, dict):
            raise ValueError('checkpoint scan lacks needed-face mapping')
        rules = payload['rules']
        fragments = payload.get('source_fragments')
        if not isinstance(fragments, list) or sorted(x.get('name') for x in fragments if isinstance(x, dict)) != sorted(rules):
            raise ValueError('timeout job source fragments do not cover assigned rules')
        groups = []
        midpoint = (len(rules)+1)//2
        for offset in (0, midpoint):
            selected = rules[offset:offset+midpoint]
            faces = []
            for rule in selected:
                if rule not in needed and rule not in scan[1]:
                    raise ValueError('current scan omits the assigned rule')
                values = needed.get(rule, [])
                if not isinstance(values, list) or any(not isinstance(x, str) or not x for x in values):
                    raise ValueError('invalid current scan target faces')
                faces.extend(values)
            if len(set(faces)) > 500:
                raise ValueError('timeout child target faces exceed bounded task limit')
            suffix = hashlib.sha256((job_id+'\0'+attempt+'\0'+str(offset)).encode()).hexdigest()[:20]
            child = dict(payload, id='timeout-child-'+suffix, rules=selected, rule_dispositions_required=selected,
                         target_faces=sorted(set(faces)),
                         source_fragments=[item for item in fragments if item['name'] in selected],
                         timeout_origin={'job_id': job_id, 'attempt_id': attempt, 'checkpoint': checkpoint})
            child.pop('reuse_archive', None)
            if (campaign/'reuse'/f"{child['id']}.json").exists():
                raise ValueError('timeout child already has a reuse instruction')
            groups.append(child)
        with queue._transaction() as db:
            current = db.execute(query, (job_id,)).fetchone()
            latest = db.execute('SELECT commit_id,evidence FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
            if not current or tuple(current) != snapshot or not latest or tuple(latest) != cp_snapshot:
                raise ValueError('timeout attempt or checkpoint changed during verification')
            base_row = db.execute('SELECT evidence FROM checkpoints WHERE commit_id=?', (snapshot[4],)).fetchone()
            if not base_row or base_row[0] != base_evidence_raw:
                raise ValueError('timeout base checkpoint changed during verification')
            if db.execute("SELECT 1 FROM events WHERE kind='rehabilitated-timeout' AND json_extract(data,'$.job_id')=?", (job_id,)).fetchone():
                raise ValueError('timeout rehabilitation already used')
            if verify(campaign, job_id, attempt, state, evidence) != proof:
                raise ValueError('timeout evidence changed during verification')
            if retire_rehabilitation.verify_scan(campaign, checkpoint, json.loads(cp_snapshot[1]), scan_proof) != scan:
                raise ValueError('checkpoint scan changed during verification')
            queue._require_absent_pid(proof['pid'])
            owned = {r[0] for r in db.execute('SELECT rule FROM rules WHERE job_id=?', (job_id,))}
            if owned != set(rules):
                raise ValueError('timeout job does not own its original rules')
            for child in groups:
                if (campaign/'reuse'/f"{child['id']}.json").exists():
                    raise ValueError('timeout child already has a reuse instruction')
                db.execute("INSERT INTO jobs(id,payload,status) VALUES (?,?,'pending')", (child['id'], json.dumps(child, sort_keys=True, separators=(',', ':'))))
                db.executemany('UPDATE rules SET job_id=? WHERE rule=? AND job_id=?',
                               [(child['id'], rule, job_id) for rule in child['rules']])
            db.execute("UPDATE jobs SET status='superseded' WHERE id=?", (job_id,))
            result = {'job_id': job_id, 'attempt_id': attempt, 'checkpoint': checkpoint,
                      'children': [child['id'] for child in groups], 'proof': proof, 'evidence': evidence}
            queue._event(db, 'rehabilitated-timeout', result)
            return result
