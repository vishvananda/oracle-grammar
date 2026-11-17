"""Small, descriptor-bound primary-cohort data for an early rejection screen."""
import hashlib
import json
from pathlib import Path

import retire as r
import retire_cohort as cohort

MAX_BYTES = 16 * 1024 * 1024
MIN_FULL_OCCURRENCES = 128


def cohort_hash(names):
    return hashlib.sha256(json.dumps(sorted(set(names)), separators=(',', ':')).encode()).hexdigest()


def build(by_name, records, oracle_index, job, full, manifest_sha):
    """Build once from the coordinator's checkpoint-keyed immutable input cache."""
    targets = job.get('target_faces', [])
    if not targets or full['closure_occurrence_count'] <= MIN_FULL_OCCURRENCES:
        return None
    primary = cohort.close_cohort(by_name, records, {'names': targets})
    r.require(set(primary['names']) <= set(full['names']), 'primary closure escaped full diagnostic cohort')
    entries = sorted((ordinal, row) for name in primary['names'] for ordinal, row in by_name[name])
    r.require(len(entries) == primary['closure_occurrence_count'], 'primary occurrence count changed')
    record_ids = sorted({row['record'] for _, row in entries})
    gold = [records[number] for number in record_ids]
    keys = {tuple(sorted(face['name'] for face in record['faces'])) for record in gold}
    r.require(all(key in oracle_index for key in keys), 'primary oracle record missing')
    # Preserve original oracle population order, including duplicate record names.
    selected = set(primary['names'])
    oracle = [value for _, value in sorted((pair for key, pairs in oracle_index.items()
                                          if selected.intersection(key) for pair in pairs),
                                           key=lambda pair: pair[0])]
    packet = {'schema': 1, 'task_id': job['id'], 'manifest_sha256': manifest_sha,
              'full_cohort_sha256': cohort_hash(full['names']),
              'full_occurrences': full['closure_occurrence_count'], 'names': primary['names'],
              'entries': [{'ordinal': ordinal, 'record': row['record'], 'face': row['face'],
                           'name': row['name'], 'text': row['text'], 'readings': row['parse']['readings']}
                          for ordinal, row in entries], 'gold': gold, 'oracle': oracle}
    # Preserve source object key order so emitted gold/oracle match the legacy path.
    encoded = json.dumps(packet, ensure_ascii=False, separators=(',', ':')).encode()+b'\n'
    # Oversize packets are never truncated; legacy full-input screening remains safe.
    return encoded if len(encoded) <= MAX_BYTES else None


def load(run, task, manifest_sha, support):
    """Read the fixed packet path, rejecting missing, substituted or stale bytes."""
    expected = task.get('screen_packet_sha256')
    if expected is None:
        return None
    r.require(isinstance(expected, str) and len(expected) == 64
              and support.get('screen_packet_sha256') == expected, 'screen packet binding mismatch')
    path = Path(run)/'screen-packet.json'
    r.require(not path.is_symlink() and path.is_file(), 'screen packet missing or unsafe')
    with path.open('rb') as stream:
        encoded = stream.read(MAX_BYTES+1)
    r.require(len(encoded) <= MAX_BYTES and hashlib.sha256(encoded).hexdigest() == expected,
              'screen packet changed or exceeds bound')
    packet = json.loads(encoded)
    r.require(packet.get('schema') == 1 and packet.get('task_id') == task['id']
              and packet.get('manifest_sha256') == manifest_sha
              and packet.get('full_cohort_sha256') == cohort_hash(task['cohort'])
              and packet.get('full_occurrences') == task['cohort_detail']['closure_occurrence_count'],
              'screen packet task or checkpoint mismatch')
    entries, names = packet.get('entries'), packet.get('names')
    r.require(isinstance(entries, list) and 0 < len(entries) <= cohort.MAX_COHORT_OCCURRENCES
              and isinstance(names, list) and set(names) <= set(task['cohort'])
              and set(task['target_faces']) <= set(names)
              and isinstance(packet.get('gold'), list) and isinstance(packet.get('oracle'), list),
              'invalid screen packet contents')
    return packet
