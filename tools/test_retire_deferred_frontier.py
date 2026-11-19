"""A refill wave only schedules disjoint, witnessed assignments."""

import unittest

from retire_deferred_frontier import _rank


def _job(job_id, rule, *faces):
    return {'id':job_id, 'rules':[rule], 'target_faces':list(faces)}


class DeferredFrontierTests(unittest.TestCase):
    def test_refill_prefers_complete_faces_and_avoids_active_work(self):
        active = [dict(_job('active', 'm_active', 'Busy'), review_origin={'job_id':'active-origin'})]
        candidates = [(_job('near', 'm_near', 'Near'), 'near-origin'),
                      (_job('ready', 'm_ready', 'Ready'), 'ready-origin'),
                      (_job('busy', 'm_busy', 'Busy'), 'different-origin'),
                      (_job('same-parent', 'm_parent', 'Other'), 'active-origin'),
                      (_job('now-unused', 'm_unused', 'Unused'), 'unused-origin'),
                      (_job('misplaced', 'm_misplaced', 'Assigned'), 'misplaced-origin')]
        needed = {'m_near':['Near'], 'm_extra':['Near'], 'm_ready':['Ready'], 'm_busy':['Busy'],
                  'm_parent':['Other'], 'm_unused':['Unused'], 'm_misplaced':['Unassigned']}
        wave = _rank(needed, {'m_unused'}, active, candidates, 1)
        self.assertEqual([item['job_id'] for item in wave], ['ready'])
        wave = _rank(needed, {'m_unused'}, active, candidates, 12)
        self.assertEqual([item['job_id'] for item in wave], ['ready','near'])

    def test_refill_chooses_only_one_child_of_each_origin_and_disjoint_faces(self):
        candidates = [(_job('a', 'm_a', 'A'), 'shared'), (_job('b', 'm_b', 'B'), 'shared'),
                      (_job('c', 'm_c', 'C'), 'different'), (_job('d', 'm_d', 'C'), 'third')]
        needed = {'m_a':['A'], 'm_b':['B'], 'm_c':['C'], 'm_d':['C']}
        wave = _rank(needed, set(), [], candidates, 12)
        self.assertEqual([item['job_id'] for item in wave], ['a','c'])
