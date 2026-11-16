import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import unittest

import retire_plan as p


class PlanTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        directory = self.root / 'grammar' / 'syntax'
        directory.mkdir(parents=True)
        (directory / 'actions.ebnf').write_text('')
        self.path = directory / 'mined.ebnf'

    def tearDown(self):
        self.tmp.cleanup()

    def write_rules(self, names, extra=''):
        self.path.write_text('\n'.join(
            '/* 1 card, level:operation, e.g. example */\n'
            '/* @sem Operation::Move{objects:$target,\n destination:Graveyard} */\n'
            f'{n} ::= "put" target "into" "graveyard"' for n in names) + extra)

    def test_exhaustive_stable_and_bounded(self):
        names = [f'm_rule_{i}' for i in range(14)]
        self.write_rules(names)
        scan = {'needed': {n: [f'face {i}'] for i, n in enumerate(names[:-1])},
                'unused': names[-1:]}
        a = p.build_plan(self.root, scan)
        b = p.build_plan(self.root, scan)
        self.assertEqual(a, b)
        self.assertEqual(a['manifest']['expected_rules'], a['manifest']['assigned_rules'])
        self.assertFalse(a['manifest']['duplicates'])
        self.assertTrue(all(len(j['rules']) <= 6 for j in a['jobs']))
        self.assertEqual(sum(j['kind'] == 'mechanical' for j in a['jobs']), 1)
        self.assertTrue(any(len(j['rules']) == 6 for j in a['jobs']))

    def test_grouping_ignores_generated_names_and_unknown_is_retained(self):
        self.write_rules(['m_apple', 't_zebra'], '\n/* @sem Mystery::Blorp */\n'
                         'm_unknown ::= "unusual"\n')
        scan = {'needed': {'m_apple': ['A'], 't_zebra': ['B'], 'm_unknown': ['C']}, 'unused': []}
        result = p.build_plan(self.root, scan)
        self.assertTrue(any(j['rules'] == ['m_apple', 't_zebra'] for j in result['jobs']))
        unknown = next(j for j in result['jobs'] if 'm_unknown' in j['rules'])
        self.assertTrue(unknown['requires_diagnosis'])
        self.assertEqual(unknown['kind'], 'author')

    def test_coarse_packing_keeps_primary_capability_and_dispositions(self):
        self.path.write_text(
            '/* @sem Instruction::[Effect{operation:Move{destination:Graveyard}}, '
            'Effect{operation:Draw{}}] */\nm_alpha ::= "put" target\n'
            '/* @sem Instruction::[Effect{operation:Move{destination:Graveyard}}, '
            'Effect{operation:Reveal{}}] */\nt_beta ::= "return" target\n'
            '/* @sem Instruction::[Effect{operation:Destroy{}}] */\nm_other ::= "destroy" target\n')
        scan = {'needed': {'m_alpha': ['A'], 't_beta': ['B'], 'm_other': ['C']}, 'unused': []}
        fine = p.build_plan(self.root, scan)
        coarse = p.build_plan(self.root, scan, coarse=True)
        self.assertEqual(coarse['packing'], 'coarse')
        self.assertEqual(len(fine['jobs']), 3)
        self.assertEqual(len(coarse['jobs']), 2)
        packed = next(j for j in coarse['jobs'] if len(j['rules']) == 2)
        self.assertEqual(packed['rules'], ['m_alpha', 't_beta'])
        self.assertEqual(packed['rule_dispositions_required'], packed['rules'])
        self.assertEqual(coarse['manifest']['expected_rules'], coarse['manifest']['assigned_rules'])

    def test_invalid_scan_rejected(self):
        self.write_rules(['m_one'])
        with self.assertRaises(ValueError):
            p.build_plan(self.root, {'needed': {}, 'unused': []})

    def test_shared_cohort_cache_is_safe_during_concurrent_eviction(self):
        index = {'0': {'name': 'A'}}
        gold = [{'faces': [{'name': 'A', 'abilities': []}]}]
        prepared = p.prepare_cohort_index(index, gold)
        scan = {'needed': {'m_one': ['A']}}
        def select(number):
            job = {'rules': ['m_one'], 'target_faces': ['A'],
                   'anchors': {'operations': [f'Operation{number % 160}']}}
            return p.select_cohort_detail(job, prepared, gold, scan)['names']
        with ThreadPoolExecutor(max_workers=12) as pool:
            self.assertTrue(all(names == ['A'] for names in pool.map(select, range(3200))))
        self.assertLessEqual(len(prepared['neighborhood_cache']), 128)

    def test_cohort_is_contained_and_no_silent_truncation(self):
        index = {str(i): {'name': n} for i, n in enumerate(['A', 'B', 'C'])}
        gold = [{'faces': [{'name': 'A', 'abilities': [{'operation': {'kind': 'Move', 'zone': 'Graveyard'}}]},
                           {'name': 'B', 'abilities': [{'operation': {'kind': 'Move', 'zone': 'Graveyard'}}]},
                           {'name': 'C', 'abilities': [{'operation': {'kind': 'Move', 'zone': 'Hand'}}]}]}]
        prepared = p.prepare_cohort_index(index, gold)
        job = {'rules': ['m_one'], 'target_faces': ['C'], 'anchors': {'operations': ['Move'], 'zones': ['Graveyard']}}
        scan = {'needed': {'m_one': ['C']}}
        self.assertEqual(p.select_cohort(job, prepared, gold, scan), ['A', 'B', 'C'])
        with self.assertRaises(p.CohortReview) as caught:
            p.select_cohort(job, prepared, gold, scan, bound=2)
        self.assertEqual(caught.exception.count, 3)
        detail = p.select_cohort_detail(job, prepared, gold, scan, bound=2)
        self.assertTrue(detail['sampled'])
        self.assertIn('C', detail['names'])
        self.assertTrue(detail['full_batch_audit_required'])
        job['anchors'] = {}
        job['syntax_neighbor_faces'] = ['B']
        detail = p.select_cohort_detail(job, prepared, gold, scan)
        self.assertEqual(detail['names'], ['B', 'C'])
        self.assertIn('shared-production', detail['scope'])
        job['target_faces'] = ['outside']
        with self.assertRaises(ValueError):
            p.select_cohort(job, prepared, gold, scan)


if __name__ == '__main__':
    unittest.main()
