#!/usr/bin/env python3
"""Export current escalations for human review without touching the work queue."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import tempfile


STATUSES = ('needs-review', 'integration-rejected', 'failed')
ROOT = re.compile(r'^\s*([A-Za-z][A-Za-z0-9_]*)\s*::')
BATCH_ID = re.compile(r'[0-9a-f]{32}')


def _hash(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def _artifact(root, value, expected=None):
    if not isinstance(value, str) or not value:
        return None
    candidate = Path(value)
    if not candidate.is_absolute():
        return None
    path = candidate.resolve()
    if not path.is_relative_to(root):
        return {'status': 'outside-campaign'}
    if not path.is_file():
        return {'path': str(path), 'status': 'missing'}
    actual = _hash(path)
    return {'path': str(path), 'sha256': actual,
            'status': 'verified' if expected and expected == actual else
                      'mismatch' if expected else 'hashed'}


def _archive(root, attempt):
    if not isinstance(attempt, str) or Path(attempt).name != attempt or attempt in ('', '.', '..'):
        return None
    metadata = root/'.cache/attempts'/attempt/'archive.json'
    if not metadata.is_file() or not metadata.resolve().is_relative_to(root):
        return None
    try:
        record = json.loads(metadata.read_text())
    except (OSError, ValueError):
        return {'status': 'invalid-metadata'}
    return _artifact(root/'.cache/archives', record.get('archive'), record.get('sha256'))


def _integration(root, identity, job_id):
    if not isinstance(identity, str) or not BATCH_ID.fullmatch(identity):
        return None, None, None
    batches = root/'.cache/batches'
    folder = batches/identity
    manifest = folder/'manifest.json'
    if not manifest.resolve().is_relative_to(batches.resolve()) or not batches.resolve().is_relative_to(root):
        return None, None, None
    artifact = _artifact(batches, str(manifest))
    if not manifest.is_file():
        return artifact, None, None
    try:
        value = json.loads(manifest.read_text())
        matches = [job for job in value.get('jobs', []) if job.get('job_id') == job_id]
        reason = matches[0].get('reason') if len(matches) == 1 else None
        validation = folder/'validation.json'
        validation_reason = None
        if validation.resolve().is_relative_to(batches.resolve()) and validation.is_file():
            validation_reason = json.loads(validation.read_text()).get('reason')
        return artifact, reason, validation_reason
    except (OSError, ValueError, TypeError, AttributeError):
        return artifact, None, None


def _lanes(status, task, outcome, model):
    text = ' '.join(str(v or '') for v in (outcome.get('reason'), model.get('reason'),
                    task.get('capability'), ' '.join(task.get('semantic_atoms', [])))).lower()
    lanes = set()
    if status == 'integration-rejected' or any(x in text for x in ('integration', 'conflict', 'merge')):
        lanes.add('integration/tooling')
    if any(x in text for x in ('operational', 'timeout', 'cpu pool', 'permission', 'helper failed')):
        lanes.add('operational')
    if any(x in text for x in ('binding', 'bind ', 'native', 'rust', 'contract', 'schema')):
        lanes.add('native-binding/Rust-contract')
    if any(x in text for x in ('print', 'reparse', 'round-trip', 'round_trip', 'exact default')):
        lanes.add('print-regression')
    if not lanes or any(x in text for x in ('syntax', 'grammar', 'production', 'phrase')):
        lanes.add('shared-syntax')
    return sorted(lanes)


def review(campaign):
    """Use only currently owned rules and their current attempt, never historical attempts."""
    root = Path(campaign).resolve()
    db = sqlite3.connect((root/'queue.sqlite').as_uri()+'?mode=ro', uri=True, isolation_level=None)
    try:
        db.execute('PRAGMA query_only=ON')
        db.execute('BEGIN')
        checkpoint = db.execute('SELECT commit_id FROM checkpoints ORDER BY seq DESC LIMIT 1').fetchone()
        rows = list(db.execute('SELECT j.id,j.payload,j.status,j.attempt_id,j.adjudication,'
                               'a.base_commit,a.result,r.rule FROM rules r JOIN jobs j ON j.id=r.job_id '
                               'LEFT JOIN attempts a ON a.id=j.attempt_id '
                               "WHERE j.status IN ('needs-review','integration-rejected','failed') "
                               'ORDER BY j.id,r.rule'))
        db.commit()
    finally:
        db.close()
    jobs = {}
    for job_id, payload, status, attempt, adjudication, base, raw, rule in rows:
        task = json.loads(payload)
        result = json.loads(raw) if raw else {}
        evidence = result.get('evidence') if isinstance(result.get('evidence'), dict) else {}
        outcomes = result.get('outcomes') if isinstance(result.get('outcomes'), dict) else {}
        outcome = outcomes.get(rule) if isinstance(outcomes.get(rule), dict) else {}
        dispositions = evidence.get('dispositions') if isinstance(evidence.get('dispositions'), dict) else {}
        model = dispositions.get(rule) if isinstance(dispositions.get(rule), dict) else {}
        fragments = task.get('source_fragments') or []
        roots = sorted({match.group(1) for frag in fragments if isinstance(frag, dict)
                        and isinstance(frag.get('sem'), str) and (match := ROOT.match(frag['sem']))})
        if job_id not in jobs:
            decision = json.loads(adjudication) if adjudication else {}
            detail = decision.get('evidence') if isinstance(decision.get('evidence'), dict) else {}
            manifest, job_reason, validation_reason = _integration(root, detail.get('manifest'), job_id)
            jobs[job_id] = {'job_id': job_id, 'status': status, 'kind': task.get('kind'), 'attempt_id': attempt,
                            'base_checkpoint': base, 'primary_modules': task.get('primarymodules') or [],
                            'semantic_roots': roots, 'rules': [],
                            'report': _artifact(root/'.cache/attempts'/str(attempt)/'evaluations',
                                                evidence.get('report')),
                            'integration_reason': detail.get('reason'),
                            'integration_manifest': manifest, 'integration_job_reason': job_reason,
                            'integration_validation_reason': validation_reason,
                            'candidate_archive': _archive(root, attempt)}
        model_status = model.get('status')
        source = 'mechanical' if task.get('kind') == 'mechanical' else 'model'
        assessment = (f'{source}-proposed-bundle-not-passed' if model_status == 'proposed'
                      and result.get('status') != 'provisional-pass'
                      else f'{source}-needs-review' if model_status == 'needs-review'
                      else 'integration-rejected' if status == 'integration-rejected'
                      else 'attempt-failed' if status == 'failed' else 'unclassified')
        jobs[job_id]['rules'].append({'rule': rule, 'disposition_source': source, 'model_status': model_status,
                                      'model_reason': model.get('reason'),
                                      'outcome_status': outcome.get('status'),
                                      'outcome_reason': outcome.get('reason'),
                                      'assessment': assessment,
                                      'suggested_lanes': _lanes(status, task, outcome, model)})
    return {'latest_checkpoint': checkpoint[0] if checkpoint else None,
            'lane_note': 'Heuristic routing suggestions, not diagnoses or acceptance decisions.',
            'jobs': list(jobs.values()), 'rule_count': len(rows)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('campaign', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = json.dumps(review(args.campaign), indent=2, sort_keys=True, allow_nan=False)+'\n'
    if args.output is None:
        print(result, end='')
        return
    root = args.campaign.resolve()
    cache_path = root/'.cache'
    cache = cache_path.resolve()
    target = args.output.resolve()
    if (cache_path.is_symlink() or not cache.is_relative_to(root) or not target.is_relative_to(cache)
            or target == cache or not target.parent.is_dir()):
        raise ValueError('output must be a file inside campaign .cache')
    fd, name = tempfile.mkstemp(prefix='.review-', dir=target.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            stream.write(result)
        os.replace(name, target)
    finally:
        Path(name).unlink(missing_ok=True)


if __name__ == '__main__':
    main()
