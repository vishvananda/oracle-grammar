import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from argparse import Namespace
from unittest.mock import patch

import retire as r
import retire_author_pool as models
import retire_campaign as campaign
import retire_dispatch as dispatch
from retire_queue import Queue


class DecisionsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name)/'decisions.json'
        self.rules = ['m_one', 'm_two']

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, rows):
        r.write(self.path, {'summary': 'diagnosed', 'rules': rows})

    def row(self, rule, status='needs-review'):
        return {'rule': rule, 'status': status,
                'reason': 'The scoped operand requires a separate grammar contract.',
                'evidence': 'packet source fragment and diagnostic report'}

    def test_requires_exactly_one_disposition_for_every_rule(self):
        self.write([self.row('m_one'), self.row('m_two', 'proposed')])
        result = dispatch.decisions(self.path, self.rules)
        self.assertEqual(set(result), set(self.rules))
        self.assertEqual(result['m_two']['status'], 'proposed')
        for bad in ([self.row('m_one')], [self.row('m_one'), self.row('m_one')],
                    [self.row('m_one'), self.row('m_unknown')]):
            self.write(bad)
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                dispatch.decisions(self.path, self.rules)
        self.write([self.row('m_one', 'accepted'), self.row('m_two')])
        with self.assertRaises(ValueError):
            dispatch.decisions(self.path, self.rules)
        short = self.row('m_one')
        short['reason'] = 'unclear'
        self.write([short, self.row('m_two')])
        with self.assertRaises(ValueError):
            dispatch.decisions(self.path, self.rules)

    def test_response_schema_constrains_disposition_fields(self):
        schema = models.response_schema(self.rules)
        self.assertEqual(set(schema['required']), {'summary', 'rules'})
        self.assertFalse(schema['additionalProperties'])
        row = schema['properties']['rules']['items']
        self.assertEqual(set(row['required']), {'rule', 'status', 'reason', 'evidence'})
        self.assertFalse(row['additionalProperties'])
        self.assertEqual(row['properties']['rule']['enum'], self.rules)
        self.assertEqual(set(row['properties']['status']['enum']), {'proposed', 'needs-review'})


class CampaignTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.base = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_prepare_preserves_exact_inventory_and_emits_status(self):
        run = self.base/'campaign'
        paths = {name: self.base/name for name in ('source', 'helpers', 'baseline', 'gold',
                                                   'oracle', 'allowed', 'main', 'target')}
        for path in paths.values():
            path.mkdir()
        job = {'id': 'bundle', 'kind': 'author', 'rules': ['m_one', 't_two'],
               'source_fragments': [{'name': 'm_one', 'sem': 'one'},
                                    {'name': 't_two', 'sem': 'two'}]}
        plan = {'jobs': [job], 'manifest': {'expected_rules': ['m_one', 't_two']}}

        def freeze(args):
            args.run.mkdir(parents=True)
            r.write(args.run/'manifest.json', {'test': 'frozen'})

        def scan(args):
            (args.run/'scan').mkdir()
            r.write(args.run/'scan/rules.json', {'needed': {}, 'unused': []})

        args = Namespace(run=run, workers=2, **paths)
        out = io.StringIO()
        with patch.object(campaign.subprocess, 'check_output', return_value='base\n'), \
             patch.object(campaign.r, 'freeze', side_effect=freeze), \
             patch.object(campaign.r, 'scan', side_effect=scan), \
             patch.object(campaign.planner, 'build_plan', return_value=plan), \
             contextlib.redirect_stdout(out):
            campaign.prepare(args)
        self.assertEqual(json.loads(out.getvalue())['rules'], 2)
        coverage = Queue(run/'queue.sqlite').coverage()
        self.assertEqual(set(coverage['rules']), {'m_one', 't_two'})
        self.assertEqual(coverage['counts'], {'pending': 2})
        self.assertEqual(coverage['checkpoint'], 'base')
        self.assertEqual(r.read(run/'config.json')['expected_rules'], ['m_one', 't_two'])
        runner = run/'runner'
        startup = subprocess.run(
            [sys.executable, '-B', str(runner/'retire_recover_full_print.py'), '--help'],
            cwd=runner, env={key: value for key, value in os.environ.items() if key != 'PYTHONPATH'},
            capture_output=True, text=True, check=True,
        )
        self.assertIn('--include-parse', startup.stdout)
        self.assertEqual(r.tree(runner), r.read(run/'runner.json'))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            campaign.status(run)
        status = json.loads(out.getvalue())
        self.assertEqual(status['rules'], 2)
        self.assertEqual(status['checkpoint'], 'base')
        self.assertEqual(status['status']['status'], 'prepared')

    def test_mechanical_splitting_preserves_ownership(self):
        job = {'id': 'mechanical', 'kind': 'mechanical', 'rules': ['m_one', 't_two'],
               'source_fragments': [{'name': 'm_one'}, {'name': 't_two'}]}
        jobs = campaign.singleton_mechanical([job])
        self.assertEqual(len(jobs), 2)
        self.assertEqual({r for j in jobs for r in j['rules']}, {'m_one', 't_two'})
        self.assertEqual(len({j['id'] for j in jobs}), 2)
        for item in jobs:
            self.assertEqual([f['name'] for f in item['source_fragments']], item['rules'])

    def test_claims_use_latest_checkpoint_and_old_attempt_keeps_base(self):
        queue = Queue(self.base/'queue.db')
        queue.add_jobs([{'id': 'one', 'rules': ['m_one']}, {'id': 'two', 'rules': ['m_two']}])
        queue.add_checkpoint('old', {'proof': 'old'})
        first = queue.claim('worker-1')
        queue.add_checkpoint('new', {'proof': 'new'}, expected_parent='old')
        second = queue.claim('worker-2')
        self.assertEqual(first['base_commit'], 'old')
        self.assertEqual(second['base_commit'], 'new')
        self.assertEqual(queue.get_attempt(first['attempt_id'])['base_commit'], 'old')
        with self.assertRaises(ValueError):
            queue.claim('worker-3', checkpoint='old')

    def test_context_loads_each_checkpoint_once(self):
        cp = {'commit': 'one', 'inputs': str(self.base/'inputs')}
        contexts = dispatch.Contexts()
        with patch.object(dispatch.w, 'payload', return_value=('index', 'gold', 'oracle')) as payload, \
             patch.object(dispatch.r, 'read', return_value={'needed': {}}) as scan, \
             patch.object(dispatch.planner, 'prepare_cohort_index', return_value='inverted') as index:
            self.assertEqual(contexts.get(cp), contexts.get(cp))
            payload.assert_called_once()
            scan.assert_called_once()
            index.assert_called_once_with('index', 'gold')


if __name__ == '__main__':
    unittest.main()
