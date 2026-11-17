"""Bounded transitive cohort closure over immutable indexed gold records."""
from collections import deque

import retire as r


MAX_BASE_COHORT_NAMES = 500
MAX_COHORT_OCCURRENCES = 1024


class CohortClosureLimit(ValueError):
    """The complete shared-face closure is too large for a bounded diagnostic."""
    def __init__(self, count, limit):
        self.count, self.limit = count, limit
        super().__init__(f'cohort closure exceeds {limit} face occurrences (at least {count}); coordinator review required')


def close_cohort(by_name, records, detail, *, max_occurrences=MAX_COHORT_OCCURRENCES, already_closed=False):
    """Close names over shared gold records without dropping duplicate occurrences."""
    if type(max_occurrences) is not int or not 1 <= max_occurrences <= MAX_COHORT_OCCURRENCES:
        raise ValueError('invalid cohort occurrence limit')
    names = detail.get('names')
    r.require(isinstance(names, list) and all(isinstance(name, str) and name for name in names),
              'cohort names must be a list of nonempty strings')
    initial = set(names)
    r.require(type(already_closed) is bool, 'already_closed must be boolean')
    if already_closed:
        r.require(len(initial) <= max_occurrences, 'closed cohort exceeds occurrence limit')
    else:
        r.require(len(initial) <= MAX_BASE_COHORT_NAMES, 'base cohort exceeds 500 distinct face names')
    r.require(initial <= by_name.keys(), 'cohort outside frozen population')
    pending = deque(sorted(initial))
    selected, record_ids, observed = set(initial), set(), set()
    occurrences = 0
    while pending:
        name = pending.popleft()
        for _, row in by_name[name]:
            record, face = row.get('record'), row.get('face')
            r.require(type(record) is int and 0 <= record < len(records)
                      and type(face) is int and 0 <= face < len(records[record]['faces'])
                      and records[record]['faces'][face]['name'] == name,
                      'frozen index address does not match gold face')
            address = (record, face)
            r.require(address not in observed, 'frozen index repeats a gold face address')
            observed.add(address)
            if record in record_ids:
                continue
            record_ids.add(record)
            faces = records[record]['faces']
            occurrences += len(faces)
            if occurrences > max_occurrences:
                raise CohortClosureLimit(occurrences, max_occurrences)
            for item in faces:
                companion = item['name']
                r.require(companion in by_name, 'gold companion outside frozen population')
                if companion not in selected:
                    selected.add(companion)
                    pending.append(companion)
    expected = {(record, face) for record in record_ids for face in range(len(records[record]['faces']))}
    r.require(observed == expected, 'frozen index omits gold face occurrences in the cohort closure')
    if already_closed:
        r.require(selected == initial, 'full diagnostic cohort is not closed')
    result = dict(detail)
    result.update(names=sorted(selected), closure_added_names=sorted(selected-initial),
                  closure_base_name_count=len(initial), closure_record_count=len(record_ids),
                  closure_occurrence_count=occurrences, closure_occurrence_limit=max_occurrences,
                  closure_scope='transitive shared-face-name closure over frozen gold records; no truncation')
    return result

