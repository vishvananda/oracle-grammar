"""The isolation planner needs a passing final union before proposing survivors."""
from contextlib import nullcontext
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch

import retire_parse_isolate as isolate


class SurvivorTests(unittest.TestCase):
    def test_added_trace_keeps_the_hint_when_the_retained_meaning_has_no_route(self):
        row = {'all': [[{'route':'retained'}],[{'route':'added'}]]}
        delta = {'retained_current_indices':[0],'added_current_indices':[1]}
        lines = [b'[{"route":"retained"}]', b'  rules []', b'[{"route":"added"}]',
                 b'  rules ["new_phrase"]']
        self.assertEqual(isolate._trace_delta(lines,row,delta),({'new_phrase'},set()))
        with self.assertRaisesRegex(ValueError,'empty trace for added native reading'):
            isolate._trace_delta(lines[:-1],row,delta)

    def test_parse_hint_routes_only_new_productions_on_the_added_meaning(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            helpers = root/'helpers'
            helpers.mkdir()
            binary = helpers/'probe'
            binary.write_bytes(b'pinned probe')
            run = root/'inputs'
            (run/'frozen').mkdir(parents=True)
            isolate.r.write(run/'frozen/build.json', {'binaries': {'probe': isolate.r.digest(binary)}})
            full = root/'full'
            (full/'checks').mkdir(parents=True)
            isolate.r.jsonl(full/'checks/rows.jsonl', [{'name':'Face', 'text':'Text.',
                                                       'all':[[{'route':'retained'}],[{'route':'added'}]]}])
            native = full/'checks/native.json'
            additions = [{'name':'Face','added_current_indices':[1],'retained_current_indices':[0]}]
            isolate.r.write(native,{'reading_preservation':{'additions':additions,'missing':[]}})
            checkpoint = {'helpers':str(helpers),'cpu_pool':str(root/'cpu'),'cpu_capacity':32}
            proposals = []
            for index, name in enumerate(('retained', 'new_phrase', 'old_phrase')):
                directory = root/str(index)/'evaluation'
                module = directory/'grammar/syntax/example.ebnf'
                module.parent.mkdir(parents=True)
                module.write_text(f'{name} ::= "text"\n')
                proposals.append(({'kind':'author'},(run,directory)))
            frozen = run/'inputs/frozen/grammar/syntax'
            frozen.mkdir(parents=True)
            (frozen/'example.ebnf').write_text('old_phrase ::= "text"\n')
            lines = [b'[probe diagnostic]', b'[{"route":"added"}]',
                     b'  rules ["common.1","new_phrase#2","old_phrase"]',
                     b'[{"route":"retained"}]', b'  rules ["common.2","retained"]']
            with (patch.object(isolate.cpu,'reserve',return_value=nullcontext(())),
                  patch.object(isolate,'_probe_lines',return_value=lines) as probe):
                self.assertEqual(isolate._parse_hints(checkpoint,run,full,proposals,[0,1,2]),[1])
                self.assertEqual(probe.call_count,1)
                isolate.r.write(native,{'reading_preservation':{'additions':additions,
                                                                'missing':[{'name':'Other','reading':0}]}})
                self.assertEqual(isolate._parse_hints(checkpoint,run,full,proposals,[0,1,2]),[1])
                self.assertEqual(probe.call_count,2)
                isolate.r.write(native,{'reading_preservation':{'additions':additions,'missing':[1]}})
                self.assertEqual(isolate._parse_hints(checkpoint,run,full,proposals,[0,1,2]),[])
                self.assertEqual(probe.call_count,2)

    def test_parse_hint_uses_the_traced_alternative_when_meanings_share_routes(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            prior = root/'prior'
            combined = root/'combined'
            proposals = [(root/f'origin-{n}',root/f'report-{n}') for n in range(3)]
            contexts = {
                prior: 'copy_part ::= old_word | old_other',
                combined: 'copy_part ::= new_word | old_word | unseen_word | old_other',
                proposals[0][1]: 'copy_part ::= old_word | old_other | new_word',
                proposals[1][1]: 'copy_part ::= old_other | old_word',
                proposals[2][1]: 'copy_part ::= unseen_word | old_word | old_other',
                **{run/'inputs/frozen': 'copy_part ::= old_word | old_other' for run,_ in proposals},
            }
            for origin, production in contexts.items():
                grammar = origin/'grammar/syntax'
                grammar.mkdir(parents=True)
                (grammar/'copy.ebnf').write_text(production+'\n')
            edges = isolate._added_alternatives(prior/'grammar',combined/'grammar',
                                                 {'copy_part.0','copy_part.1','copy_part.9'})
            self.assertEqual(edges,{('copy_part','new_word')})
            self.assertEqual([i for i,proposal in enumerate(proposals)
                              if isolate._proposal_added_alternative(*proposal,edges)], [0])

            (combined/'checks').mkdir()
            isolate.r.jsonl(combined/'checks/rows.jsonl', [
                {'name':'Face','text':'Text.','all':[[{'reading':0}],[{'reading':1}]]},
            ])
            isolate.r.write(combined/'checks/native.json', {'reading_preservation':{'additions':[
                {'name':'Face','retained_current_indices':[0],'added_current_indices':[1]},
            ]}})
            # The unique route of the added reading belongs to an existing
            # alternative. The newly introduced branch lies on both traces.
            routes = [b'[{"reading":0}]', b'  rules ["copy_part.0","known_route.0"]',
                      b'[{"reading":1}]', b'  rules ["copy_part.0","existing_alternate.0"]']
            archive=(prior/'frozen')
            archive.mkdir()
            (archive/'grammar').symlink_to(prior/'grammar', target_is_directory=True)
            binary=root/'probe'
            binary.write_bytes(b'frozen helper')
            config={'inputs':str(prior),'cpu_pool':str(root/'cpu'),'cpu_capacity':32}
            candidates=[({'kind':'author'},entry) for entry in proposals]
            with (patch.object(isolate,'_trace_binary',return_value=(binary,isolate.r.digest(binary))),
                  patch.object(isolate.cpu,'reserve',return_value=nullcontext(())),
                  patch.object(isolate,'_probe_lines',return_value=routes)):
                self.assertEqual(isolate._parse_hints(config,prior,combined,candidates,[0,1,2]),[0])

    def test_print_hint_requires_a_unique_new_production_in_the_changed_trace(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            helpers = root/'helpers'
            helpers.mkdir()
            binary = helpers/'probe'
            binary.write_bytes(b'pinned probe')
            run = root/'inputs'
            (run/'frozen').mkdir(parents=True)
            isolate.r.write(run/'frozen/build.json', {'binaries': {'probe': isolate.r.digest(binary)}})
            full = root/'full'
            (full/'checks').mkdir(parents=True)
            isolate.r.jsonl(full/'checks/rows.jsonl', [
                {'name': 'Face', 'text': 'Old.', 'first': 'New.', 'readings': 1},
            ])
            checkpoint = {'helpers': str(helpers), 'cpu_pool': str(root/'cpu'), 'cpu_capacity': 32}
            failures = [{'face': 'Face', 'lost': ['exact_default']}]
            proposals = []
            for index, grammar in enumerate(('unchanged ::= "word"','new_phrase ::= "new"')):
                directory = root/str(index)/'evaluation'
                module = directory/'grammar/syntax/example.ebnf'
                module.parent.mkdir(parents=True)
                module.write_text(grammar+'\n')
                proposals.append(({'kind': 'author'}, (run, directory)))
            with (patch.object(isolate.cpu, 'reserve', return_value=nullcontext(())),
                  patch.object(isolate, '_probe_rules', side_effect=[
                      {'new_phrase','same'}, {'same'}, {'new_phrase'}, set()])):
                self.assertEqual(isolate._print_hints(checkpoint, run, full, proposals, [0, 1], failures), [1])
                self.assertEqual(isolate._print_hints(checkpoint, run, full, proposals, [0], failures), [])

    def test_multiple_print_losses_route_new_and_removed_productions(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            probe = root/'probe'
            probe.write_bytes(b'pinned probe')
            full = root/'full'
            (full/'checks').mkdir(parents=True)
            isolate.r.jsonl(full/'checks/rows.jsonl', [
                {'name':'New route', 'text':'Old.', 'first':'New.', 'readings':1},
                {'name':'Removed route', 'text':'Exact.', 'first':'Other.', 'readings':1},
            ])
            failures = [{'face':name, 'lost':['exact_default']} for name in ('New route','Removed route')]
            checkpoint = {'inputs':str(root/'accepted'),'cpu_pool':str(root/'cpu'),'cpu_capacity':32}
            proposals = [({'kind':'author','rules':['m_removed']},(root,root/'one')),
                         ({'kind':'author','rules':['m_other']},(root,root/'two'))]
            with (patch.object(isolate.cpu,'reserve',return_value=nullcontext(())),
                  patch.object(isolate,'_trace_binary',return_value=(probe,isolate.r.digest(probe))),
                  patch.object(isolate,'_probe_rules',side_effect=[
                      {'new_phrase'},{'old_phrase'},{'shared_typing'},{'shared_typing'},{'m_removed'},
                  ]),
                  patch.object(isolate,'_new_productions',side_effect=lambda run,directory,wanted:
                               wanted if directory.name=='two' else set())):
                self.assertEqual(isolate._print_hints(checkpoint,root,full,proposals,[0,1],failures),[0,1])

    def test_broad_print_recovery_traces_only_eight_faces(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            probe = root/'probe'
            probe.write_bytes(b'pinned probe')
            full = root/'full'
            (full/'checks').mkdir(parents=True)
            names = [f'Face {index}' for index in range(26)]
            isolate.r.jsonl(full/'checks/rows.jsonl', [{'name':name} for name in names])
            failures = [{'face':name, 'lost':['exact_default']} for name in names]
            cp = {'cpu_pool':str(root/'cpu'),'cpu_capacity':32}
            with (patch.object(isolate.cpu,'reserve',return_value=nullcontext(())),
                  patch.object(isolate,'_trace_binary',return_value=(probe,isolate.r.digest(probe))),
                  patch.object(isolate,'_print_face_hints',return_value=[1]) as trace):
                self.assertEqual(isolate._print_hints(cp,root,full,[],[1],failures),[1])
                self.assertEqual([call.args[3]['name'] for call in trace.call_args_list],names[:8])

    def test_completed_prefix_reuses_the_longest_exact_order_and_checks_its_bytes(self):
        with TemporaryDirectory() as temporary:
            root = Path(temporary)
            seen = {}
            for index, selection in enumerate(((0,), (0, 1, 4), (0, 4))):
                trial = root/str(index)
                grammar = trial/'grammar'
                grammar.mkdir(parents=True)
                (grammar/'rules.ebnf').write_text(str(selection))
                seen[selection] = {'root': trial, 'tree': isolate.r.tree(grammar)}
            self.assertEqual(isolate._completed_prefix([0, 1, 2, 4, 5], seen),
                             (root/'0/grammar', [1, 2, 4, 5]))
            self.assertEqual(isolate._completed_prefix([0, 1, 4, 5], seen),
                             (root/'1/grammar', [5]))
            (root/'1/grammar/rules.ebnf').write_text('tampered')
            with self.assertRaisesRegex(ValueError, 'cached grammar prefix changed'):
                isolate._completed_prefix([0, 1, 4, 5], seen)

    def test_retry_hints_require_a_failing_singleton_with_the_pinned_prefix(self):
        jobs = [{'job_id': str(i)} for i in range(6)]
        failure = {'result': {'failures': ['regression']}}
        passed = {'result': {'failures': []}}
        seen = {(0, 1): passed, (0, 1, 2, 3): failure, (0, 1, 2): failure,
                (0, 1, 4): passed, (1, 5): failure}
        self.assertEqual(isolate._witnessed_suspects(seen, [0, 1], jobs), ['2'])

    def test_suspect_skips_bisection_when_the_remaining_union_passes(self):
        trials = []

        def trial(indices):
            trials.append(indices)
            return {'failures': ['regression'] if 2 in indices else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0], suspect=2), [0, 1, 3])
        self.assertEqual(trials, [[0, 1, 3]])

    def test_incorrect_suspect_falls_back_to_the_actual_contributor(self):
        def trial(indices):
            return {'failures': ['regression'] if 2 in indices else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0], suspect=1), [0, 1, 3])

    def test_multiple_suspects_exclude_only_reproduced_failures(self):
        trials = []

        def trial(indices):
            trials.append(indices)
            return {'failures': ['regression'] if {2, 4}.intersection(indices) else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3, 4], trial, [0], suspects=[1, 2, 4]), [0, 1, 3])
        self.assertEqual(trials, [[0, 1], [0, 2], [0, 4], [0, 1, 3]])

    def test_one_exact_default_loss_checks_the_survivor_union_before_other_branches(self):
        checked = set()
        candidates = list(range(48))

        def trial(indices):
            checked.add(tuple(indices))
            return {'failures': [{'face': 'Fixture', 'lost': ['exact_default']}] if 18 in indices else []}

        remaining = isolate._survivors(candidates, trial, [])
        self.assertEqual(remaining, [index for index in candidates if index != 18])
        self.assertIn(tuple(remaining), checked)
        self.assertLessEqual(len(checked), 9)

    def test_same_print_loss_uses_full_split_after_the_first_exclusion(self):
        checked = []
        candidates = list(range(8))

        def trial(indices):
            checked.append(tuple(indices))
            return {'failures': [{'face': 'Fixture', 'lost': ['exact_default']}]
                    if {1, 5, 6}.intersection(indices) else []}

        remaining = isolate._survivors(candidates, trial, [0])
        self.assertEqual(remaining, [0, 2, 3, 4, 7])
        self.assertIn((0, 2, 3, 4, 5, 6, 7), checked)
        self.assertEqual(checked[-1], tuple(remaining))

    def test_second_failure_crosses_halves_after_first_culprit_is_removed(self):
        checked = []

        def trial(indices):
            self.assertIn(0, indices)
            self.assertEqual(indices, sorted(set(indices)))
            checked.append(indices)
            failed = 1 in indices or {2, 4} <= set(indices)
            return {'failures': ['regression'] if failed else []}

        remaining = isolate._survivors([0, 1, 2, 3, 4], trial, [0])
        self.assertEqual(remaining, [0, 3, 4])
        self.assertIn([0, 2, 3, 4], checked)
        self.assertEqual(trial(remaining)['failures'], [])

    def test_one_side_of_an_interaction_can_be_readmitted(self):
        def trial(indices):
            return {'failures': ['regression'] if {2, 3} <= set(indices) else []}

        self.assertEqual(isolate._survivors([0, 1, 2, 3], trial, [0]), [0, 1, 2])

    def test_retry_reuses_independent_witness_and_checks_the_surviving_union(self):
        with TemporaryDirectory() as temporary:
            campaign = Path(temporary)
            inputs = campaign/'inputs'
            grammar = inputs/'frozen/grammar'
            grammar.mkdir(parents=True)
            (grammar/'card.ebnf').write_text('card ::= "fixture"\n')
            folder = campaign/'batch'
            folder.mkdir()
            for name in ('manifest.json', 'validation.json', 'repository-check.log'):
                (folder/name).write_text('{}\n')
            jobs = [{'job_id': str(i), 'attempt_id': str(i), 'kind': 'author', 'rules': [str(i)]}
                    for i in range(4)]
            cp = {'commit': 'accepted', 'cpu_pool': str(campaign/'cpu'), 'cpu_capacity': 4}
            manifest = {'id': 'batch', 'base_commit': 'accepted', 'commit': 'rejected',
                        'grammar': isolate.r.tree(grammar)}
            archived = (folder, manifest, jobs, cp, inputs, inputs, ['Fixture'], {})
            scratch = campaign/'.cache/proof'
            trials = []

            def checked(_, _control, root):
                indices = isolate.r.read(root/'identity.json')['indices']
                trials.append(indices)
                result = {'faces': 1, 'failures': ['regression'] if 1 in indices else []}
                check = root/'checks'
                check.mkdir()
                for name in isolate.readmission.ARTIFACTS:
                    (check/name).write_text('')
                isolate.r.write(check/'slice.json', {'cards': ['Fixture']})
                isolate.r.write(root/'result.json', result)
                return result

            with (
                patch.object(isolate, '_archived', return_value=archived),
                patch.object(isolate, '_proposals', return_value=[(i,) for i in range(4)]),
                patch.object(isolate, '_apply'),
                patch.object(isolate.batch, '_control_setup', return_value={'occurrences': 1, 'names': ['Fixture']}),
                patch.object(isolate.cpu, 'reserve', side_effect=lambda *a, **k: nullcontext()),
                patch.object(isolate, '_check', side_effect=checked),
            ):
                result = isolate.isolate(campaign, 'batch', scratch, 5, witness_retry=3)
                original_trials = trials.copy()
                trials.clear()
                refreshed = campaign/'.cache/refreshed'
                anchors = result['evidence']['isolation']
                survivors = isolate.isolate(campaign, 'batch', refreshed, 3, unavailable=['2'],
                                            preferred=['0', '3'], retry_anchors=anchors)
                refreshed_trials = trials.copy()
                trials.clear()
                (scratch/'base/checks/score.log').write_text('changed receipt')
                fallback = isolate.isolate(campaign, 'batch', campaign/'.cache/fallback', 10,
                                           unavailable=['2'], preferred=['0', '1'], retry_anchors=anchors)
            self.assertEqual(result['jobs'], ['0', '2', '3'])
            self.assertEqual(original_trials, [[], [0, 1, 2, 3], [0, 1], [0], [1], [0, 2, 3]])
            proof = isolate.r.read(scratch/'isolation.json')
            self.assertEqual(proof['excluded_jobs'], ['1'])
            self.assertEqual(proof['controls']['base']['result']['path'], str(scratch/'base/result.json'))
            self.assertEqual(proof['controls']['full']['result']['path'], str(scratch/'full/result.json'))
            self.assertEqual(survivors['jobs'], ['0', '3'])
            self.assertEqual(refreshed_trials, [[0, 3]])
            self.assertTrue((campaign/'.cache/fallback/rejected-retry-anchors.json').is_file())
            self.assertTrue(isolate.r.read(refreshed/'routing.json')['preferred_passed'])
            self.assertEqual(fallback['jobs'], ['0', '3'])
            self.assertEqual(trials[:3], [[], [0, 1, 2, 3], [0, 1]])
            proof = isolate.r.read(refreshed/'isolation.json')
            self.assertEqual(proof['excluded_jobs'], ['1', '2'])
