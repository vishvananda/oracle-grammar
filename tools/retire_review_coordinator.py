"""Validate explicit coordinator supplements for the read-only review export."""
import hashlib
import json
from pathlib import Path
import re

MAX_FILE = 64 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
MAX_FILES = 4096
MAX_INDICES = 256
HASH = re.compile(r'[0-9a-f]{64}\Z')


def _json(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate coordinator JSON key')
            result[key] = value
        return result
    return json.loads(data, object_pairs_hook=unique)


class Evidence:
    def __init__(self, root, roots):
        self.root = root.resolve()
        cache = self.root / '.cache'
        if cache.is_symlink() or not cache.is_dir() or cache.resolve() != cache:
            raise ValueError('campaign cache must be a canonical directory')
        if not isinstance(roots, list) or len(roots) > 16:
            raise ValueError('invalid coordinator evidence roots')
        self.roots = [cache]
        seen = set()
        for value in roots:
            if not isinstance(value, str) or len(value) > 4096:
                raise ValueError('invalid coordinator evidence root')
            path = Path(value)
            if (not path.is_absolute() or str(path) != value or path == Path('/')
                    or path.is_symlink() or not path.is_dir() or path.resolve() != path
                    or path in seen):
                raise ValueError('coordinator evidence root must be a unique canonical directory')
            seen.add(path)
            self.roots.append(path)
        self.total = 0
        self.cache = {}

    def read(self, path, digest, limit=MAX_FILE):
        if not isinstance(path, str) or not Path(path).is_absolute() or not HASH.fullmatch(str(digest)):
            raise ValueError('invalid coordinator evidence specification')
        source = Path(path)
        resolved = source.resolve()
        if (source.is_symlink() or not source.is_file() or resolved != source
                or not any(resolved.is_relative_to(root) for root in self.roots)):
            raise ValueError('coordinator evidence path is outside allowed roots')
        key = (str(resolved), digest)
        if key in self.cache:
            data = self.cache[key]
            if len(data) > limit:
                raise ValueError('coordinator evidence exceeds limit')
            return data
        if len(self.cache) >= MAX_FILES or source.stat().st_size > limit:
            raise ValueError('coordinator evidence exceeds limit')
        data = source.read_bytes()
        self.total += len(data)
        if len(data) > limit or self.total > MAX_TOTAL or hashlib.sha256(data).hexdigest() != digest:
            raise ValueError('coordinator evidence hash or size differs')
        self.cache[key] = data
        return data

    def spec(self, value, limit=MAX_FILE):
        if not isinstance(value, dict):
            raise ValueError('coordinator evidence specification missing')
        return self.read(value.get('path'), value.get('sha256'), limit)

    def nested(self, value):
        if isinstance(value, dict):
            if 'path' in value and 'sha256' in value:
                self.spec(value)
            for child in value.values():
                self.nested(child)
        elif isinstance(value, list):
            for child in value:
                self.nested(child)


def _rules(index):
    rules = index.get('rules') if 'rules' in index else [index.get('rule')]
    if (not isinstance(rules, list) or not rules or len(rules) > 64
            or any(not isinstance(rule, str) or not rule for rule in rules)
            or len(set(rules)) != len(rules)):
        raise ValueError('invalid coordinator rules')
    return rules


def _nonempty(value):
    return isinstance(value, str) and bool(value.strip())


def prepare(root, db, manifest_path, manifest_sha):
    """Verify all supplements within the caller's current read-only DB snapshot."""
    source = Path(manifest_path)
    if (not source.is_absolute() or source.is_symlink() or not source.is_file()
            or source.resolve() != source or source.stat().st_size > 4 * 1024 * 1024
            or not HASH.fullmatch(str(manifest_sha))):
        raise ValueError('invalid coordinator manifest specification')
    data = source.read_bytes()
    if len(data) > 4 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != manifest_sha:
        raise ValueError('coordinator manifest hash or size differs')
    manifest = _json(data)
    entries = manifest.get('indices') if isinstance(manifest, dict) else None
    if (not isinstance(entries, list) or not entries or len(entries) > MAX_INDICES
            or manifest.get('kind') != 'private-coordinator-triage-join'):
        raise ValueError('invalid coordinator manifest')
    evidence = Evidence(root, manifest.get('evidence_roots'))
    supplements = {}
    seen_rules = set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError('invalid coordinator manifest entry')
        index = _json(evidence.spec(entry.get('index'), 1024 * 1024))
        review = _json(evidence.spec(entry.get('review'), 8 * 1024 * 1024))
        if (not isinstance(index, dict) or not isinstance(review, dict)
                or index.get('kind') != 'coordinator-review-supplement-index'
                or index.get('acceptance') is not False or index.get('requires_deeper_review') is not True
                or index.get('queue_changes') is not False or review.get('acceptance') is not False
                or index.get('evidence') != entry.get('review')
                or not _nonempty(index.get('status')) or not _nonempty(index.get('scope'))
                or not _nonempty(index.get('model_disposition'))):
            raise ValueError('coordinator index or review lacks explicit judgment')
        job, attempt, rules = index.get('job_id'), index.get('attempt_id'), _rules(index)
        if (not _nonempty(job) or Path(job).name != job or not _nonempty(attempt)
                or Path(attempt).name != attempt or entry.get('job_id') != job
                or entry.get('attempt_id') != attempt or entry.get('rules') != rules):
            raise ValueError('coordinator manifest binding differs')
        row = db.execute('SELECT a.job_id,a.base_commit,a.result,j.payload FROM attempts a '
                         'JOIN jobs j ON j.id=a.job_id WHERE a.id=?', (attempt,)).fetchone()
        if not row or row[0] != job or not row[2]:
            raise ValueError('coordinator original attempt is unavailable')
        result, payload = json.loads(row[2]), json.loads(row[3])
        if (hashlib.sha256(row[2].encode()).hexdigest() != entry.get('attempt_result_sha256')
                or result.get('status') != 'needs-review' or not isinstance(result.get('evidence'), dict)
                or result['evidence'].get('dispositions') or set(payload.get('rules', [])) != set(rules)
                or row[1] != index.get('attempt_base_checkpoint', index.get('source_checkpoint'))):
            raise ValueError('coordinator original outcome or assignment differs')
        descriptor = root/'.cache/attempts'/attempt/'tasks'/f'{job}.json'
        support = root/'.cache/attempts'/attempt/'support.json'
        task = _json(evidence.read(str(descriptor), entry.get('descriptor_sha256'), 1024 * 1024))
        support_data = _json(evidence.read(str(support), entry.get('support_sha256'), 1024 * 1024))
        if (task.get('id') != job or task.get('rules') != payload.get('rules')
                or support_data.get('tasks', {}).get(job) != entry['descriptor_sha256']):
            raise ValueError('coordinator original descriptor or support differs')
        evidence.nested(review)
        for path, digest in review.get('input_hashes', {}).items():
            evidence.read(path, digest)
        if review.get('job_id') is not None:
            details = review.get('rules')
            if (review.get('kind') != 'coordinator-review-supplement'
                    or review.get('job_id') != job or review.get('attempt_id') != attempt
                    or review.get('source_checkpoint', review.get('current_checkpoint')) != index.get('source_checkpoint')
                    or review.get('attempt_base_checkpoint', row[1]) != row[1]
                    or not isinstance(details, list) or len(details) != len(rules)
                    or any(not isinstance(value, dict) for value in details)
                    or {value.get('rule') for value in details} != set(rules)
                    or not isinstance(review.get('untested_gates'), list)
                    or not review['untested_gates'] or any(not _nonempty(x) for x in review['untested_gates'])):
                raise ValueError('coordinator review binding or untested gates differ')
            findings = {}
            for value in details:
                judgment = value.get('coordinator_triage')
                if judgment is None:
                    judgment = {key: value[key] for key in ('finding', 'capability', 'candidate_production', 'module') if key in value}
                if not isinstance(judgment, dict) or not _nonempty(judgment.get('finding')):
                    raise ValueError('coordinator review lacks an explicit finding')
                findings[value['rule']] = judgment
            untested = review['untested_gates']
        else:
            if (review.get('kind') != 'coordinator-review-supplement' or len(rules) != 1
                    or review.get('rule') != rules[0] or review.get('removed') != rules
                    or index['status'] != 'coordinator-triaged: seeded parse dependency'
                    or review.get('results', {}).get('baseline', {}).get('native_exit') != 0
                    or not review.get('results', {}).get('removed', {}).get('rows')):
                raise ValueError('invalid singleton coordinator finding')
            findings = {rules[0]: {'finding': index['status']}}
            untested = ['authored candidate', 'fresh compositional family', 'edited operands',
                        'repository gates', 'full allowed-population audit']
        for rule in rules:
            key = (job, attempt, rule)
            if key in supplements or rule in seen_rules:
                raise ValueError('duplicate coordinator binding')
            seen_rules.add(rule)
            supplements[key] = {'status': index['status'], 'origin_job_id': job, 'origin_attempt_id': attempt,
                                'source_checkpoint': index['source_checkpoint'], 'attempt_base_checkpoint': row[1],
                                'scope': index['scope'], 'finding': findings[rule], 'untested_gates': untested,
                                'acceptance': False, 'requires_deeper_review': True,
                                'model_disposition': index['model_disposition'],
                                'index': entry['index'], 'review': entry['review']}
    return supplements


def attach(value, supplements, ownership):
    remaining = set(supplements)
    for job in value['jobs']:
        origin = job.get('review_origin')
        if job.get('attempt_id') is not None:
            identity = (job['job_id'], job['attempt_id'])
        elif job.get('review_origin_valid') is True and isinstance(origin, dict):
            identity = (origin.get('job_id'), origin.get('attempt_id'))
        else:
            continue
        for rule in job['rules']:
            key = (*identity, rule['rule'])
            if key not in supplements:
                continue
            if rule.get('model_status') is not None or rule.get('assessment') != 'unclassified':
                raise ValueError('coordinator supplement conflicts with model disposition')
            rule['coordinator_triage'] = supplements[key]
            remaining.remove(key)
    unmatched = []
    for job_id, attempt, rule in sorted(remaining):
        current = ownership.get(rule)
        reason = ('no-current-owner' if current is None else
                  'outside-review-lanes' if current['status'] not in ('needs-review', 'integration-rejected', 'failed') else
                  'current-attempt-differs' if current['attempt_id'] is not None else 'unverified-origin')
        unmatched.append({'origin_job_id': job_id, 'origin_attempt_id': attempt, 'rule': rule,
                          'reason': reason, 'current_owner': current})
    value['coordinator_supplements'] = {'matched_rules': len(supplements) - len(remaining),
                                        'unmatched': unmatched}
    return value
