import json
from pathlib import Path
import signal
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

import retire as r
import retire_author_pool as author


class AuthorLaunchTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name) / 'attempt'
        (self.run / 'tasks').mkdir(parents=True)
        (self.run / 'workers').mkdir()
        self.cpu = Path(self.tmp.name) / 'campaign' / 'cpu'
        self.task = {'goal': 'compose', 'rules': ['m_one'], 'write_files': ['grammar/x'],
                     'fixture': 'families/example.json', 'edits': '.cache/edits.json',
                     'model': 'gpt-6.1-terra', 'effort': 'high', 'full_pass': True,
                     'checkout': str(Path(self.tmp.name) / 'checkout'),
                     'packet': str(self.run / 'packet.json'), 'cpu_pool': str(self.cpu)}
        r.write(self.run / 'tasks/task.json', self.task)

    def spawn(self, cmd, **kwargs):
        kwargs['stdout'].write('{"type":"thread.started"}\n')
        kwargs['stdout'].flush()
        return self.process

    def test_launch_grants_only_cpu_pool_and_private_checkout(self):
        self.process = Mock(pid=123, returncode=0)
        self.process.poll.return_value = None
        evaluation = Mock(returncode=0, stdout=json.dumps({'passed': True, 'directory': 'proof'}), stderr='')
        with patch.object(author.subprocess, 'Popen', side_effect=self.spawn) as start, \
                patch.object(author.subprocess, 'run', return_value=evaluation) as evaluate:
            receipt = author.launch(self.run, 'task')
        cmd = start.call_args.args[0]
        self.assertEqual([cmd[i+1] for i, arg in enumerate(cmd) if arg == '--add-dir'], [str(self.cpu)])
        self.assertEqual(cmd[cmd.index('--cd')+1], self.task['checkout'])
        self.assertNotIn(str(self.run), [cmd[i+1] for i, arg in enumerate(cmd) if arg == '--add-dir'])
        self.assertEqual(receipt['status'], 'completed')
        text = (self.run/'workers/task/prompt.txt').read_text()
        self.assertIn('never zip the two arrays', text)
        self.assertIn('record/face addresses independently', text)
        self.assertIn('baseline text for original family wording', text)
        self.assertIn('acquire CPU leases', text)
        self.assertIn('never invoke the\nprivate native helpers', text)
        self.assertIn('actual producer and destination', text)
        self.assertIn('Transition(@prior, After) alone does not justify', text)
        self.assertIn('scope\nthe whole producer-consumer contract', text)
        self.assertIn('reference-review or equivalence-needed with evidence', text)
        self.assertIn('Do not\nsuppress readings by first match', text)
        self.assertIn('Exact keyed preservation does not waive default-print', text)
        command = [sys.executable, str(self.run/'support/runner/retire_author.py'), str(self.run), 'task']
        self.assertIn(json.dumps([*command, '--preview']), text)
        self.assertIn('The coordinator independently checks the full assigned cohort and edited operands', text)
        self.assertIn('Use proposed only when the target preview passes', text)
        self.assertEqual(evaluate.call_args.args[0], [*command, '--coordinator'])

    def test_pilot_prompt_requests_full_diagnostic(self):
        r.write(self.run/'tasks/task.json', {**self.task, 'full_pass': False})
        text = author.prompt(self.run, 'task')
        command = [sys.executable, str(self.run/'support/runner/retire_author.py'), str(self.run), 'task']
        self.assertIn(json.dumps(command), text)
        self.assertNotIn('--preview', text)

    def test_dependency_retry_prompt_asks_to_fix_failed_preview_edits(self):
        r.write(self.run/'tasks/task.json', {**self.task, 'preview_edited_operands': True})
        text = author.prompt(self.run, 'task')
        self.assertIn('then your whole-card operand edits', text)
        self.assertIn('Fix a bounded edited-operand refusal or print failure', text)

    def test_budget_prompt_keeps_one_round_by_default(self):
        text = author.prompt(self.run, 'task')
        self.assertIn('Allow one repair round', text)
        self.assertNotIn('does not count as a diagnostic attempt', text)

    def test_repair_until_budget_prompt_uses_the_whole_budget(self):
        r.write(self.run/'tasks/task.json', {**self.task, 'repair_until_budget': True,
                                            'max_diagnostic_attempts': 5, 'wall_budget_seconds': 900})
        text = author.prompt(self.run, 'task')
        self.assertNotIn('Allow one repair round', text)
        self.assertIn('most 5 diagnostic attempts and 15 minutes', text)
        self.assertIn('does not count as a diagnostic attempt', text)
        self.assertIn('do not start a diagnostic after minute 12', text)
        self.assertIn('Never weaken a check', text)

    def test_campaign_slot_covers_model_but_ends_before_coordinator(self):
        campaign = self.cpu.parent
        campaign.mkdir()
        startup = campaign/'startup.lock'
        r.write(campaign/'config.json', {'workers': 1})
        self.process = Mock(pid=123, returncode=0)
        self.process.poll.return_value = None

        def start(cmd, **kwargs):
            self.assertEqual(len(kwargs['pass_fds']), 1)
            with self.assertRaises(BlockingIOError):
                with author.retire_cpu.reserve(campaign/'model-slots', 1, 1, wait=False):
                    self.fail('another author acquired the campaign slot during the model run')
            return self.spawn(cmd, **kwargs)

        def evaluate(*args, **kwargs):
            self.assertEqual(kwargs['pass_fds'], ())
            with author.retire_cpu.reserve(campaign/'model-slots', 1, 1, wait=False):
                return Mock(returncode=0, stdout=json.dumps({'passed': True, 'directory': 'proof'}), stderr='')

        with patch.object(author.subprocess, 'Popen', side_effect=start), \
                patch.object(author.subprocess, 'run', side_effect=evaluate):
            self.assertEqual(author.launch(self.run, 'task', startup_lock=startup)['status'], 'completed')

    def test_start_callback_failure_stops_spawned_process(self):
        self.process = Mock(pid=123, returncode=None)
        self.process.poll.return_value = None
        with patch.object(author.subprocess, 'Popen', side_effect=self.spawn), \
                patch.object(author.os, 'killpg') as kill:
            with self.assertRaisesRegex(RuntimeError, 'callback'):
                author.launch(self.run, 'task', on_started=Mock(side_effect=RuntimeError('callback')))
        kill.assert_called_once_with(123, signal.SIGTERM)
        self.process.wait.assert_called_once_with(timeout=30)


if __name__ == '__main__':
    unittest.main()
