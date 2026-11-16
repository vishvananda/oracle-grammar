# Shared grammar authoring pilot

Three Luna Max and three Terra High workers authored the same three task bundles
from accepted mechanical checkpoint `40157b9`. All six finished within their
25-minute budgets. The trial used three concurrent external CLI sessions,
private checkouts, identical frozen helpers/inputs, fixed write sets and at most
eight diagnostic attempts per worker. No worker compiled Rust or changed model
or reference data. First-model ordering alternated across task pairs.

| Measurement | Luna Max | Terra High |
| --- | ---: | ---: |
| Median worker turnaround | 17m 31s | 3m 54s |
| Estimated Standard API cost, all three workers | $0.27192 | $0.59945 |
| Initial bounded candidates passing | 3 / 3 | 3 / 3 |
| Candidates passing expanded family review | 2 / 3 | 2 / 3 |
| Worker diagnostic attempts | 10 | 8 |
| Input tokens, including cached input | 14,809,440 | 6,482,548 |
| Cached input tokens | 14,290,816 | 6,296,192 |
| Output tokens, including reasoning | 154,295 | 19,657 |

Terra's median was about 4.49 times faster at 2.20 times the API-equivalent cost.
This is a small observational authoring pilot, not a model ranking or an engine
speed benchmark. The requested effort settings differ. Timings include prototype
workflow friction; fresh model runs have not measured the subsequent tooling fixes.

## Per-task results

| Shared capability | Luna time / API estimate | Terra time / API estimate | Expanded review |
| --- | ---: | ---: | --- |
| Mixed turn/event trigger headers | 15m 55s / $0.07928 | 4m 02s / $0.19783 | Both pass after coordinator hook registration |
| AnyNumber library searches | 20m 24s / $0.10907 | 3m 31s / $0.16714 | Terra passes; Luna loses two exact-default spellings |
| Optional graveyard creature/planeswalker targets | 17m 31s / $0.08357 | 3m 54s / $0.23448 | Luna passes; Terra loses eleven exact-default spellings |

The expanded cohorts were selected from the explicitly allowed population using
the original reference IR: mixed event unions containing turn and other events,
AnyNumber search choices, and faces containing both graveyard targets and
battlefield moves. Each cohort also retained the original diagnostic controls.
With companions and duplicate occurrences, these covered 81, 97 and 503 faces.
The same expanded population was used for both models. These are conservative
family checks, not complete dependency certificates or full-population acceptance.

All six original diagnostic cohorts passed, including every native reading,
baseline-successful print gate, authored compositional family and actual
reference-IR operand edits. The coordinator reran stronger checks that also
native-validated edited IR. Expanded review then caught:

- Luna search: lost exact-default wording for Iname, Death Aspect and Scouting Trek.
- Terra targeting: lost exact-default wording on eleven neighboring cards,
  including Call of the Death-Dweller, Return to the Ranks and Reveillark.

The expanded failures are printing regressions; the review preserved native
readings. They still block acceptance. Counting either model's first successful
cohort as a completed retirement would have hidden these defects. Neither failed
variant is included in the selected integration candidate.

## Workflow defects and fixes

Both header workers registered their factored shared trigger through the mined
dispatcher. The initial write set excluded the ordinary root module and the
prototype lacked a structured registration request. Prefix-only compositional
testing did not detect the misplaced registration. The coordinator migrated both
registrations identically into the ordinary ability list, without changing their
authored semantics, and reran the 81-face review. Both passed. This correction
is coordinator work, not additional model-authored progress.

Future workers use bounded, typed `.cache/author-hooks.json` requests. The runner
applies these to a private diagnostic grammar and records the tested hash. Seeded
files must match canonical assigned-rule removal, excluding blank-line differences;
handwritten entries hidden in mined dispatchers are rejected.

Luna's search worker also damaged semantic comments and a hook during manual
seed removal. The loader rejected those attempts. The new `--remove-assigned`
command reconstructs seeded files from the immutable base, removes the fixed
rule bundle with the existing remover, and saves private backups. It preserves
authored modules and removes the need for workers to edit huge hook lines.

Other improvements retain invocation receipts, isolate coordinator evaluation
processes and inherited leases, pin the authoring runner, and validate edited
IR independently. The scheduler now starts a pair's second model when a slot
becomes free, rather than waiting for the whole first wave. The measured trial
kept its original frozen wave launcher; no queue-speed improvement is claimed.

The authoring tooling's 113 Python tests pass, as do hygiene, pin and diff checks.
The previously paused R36 source changes are preserved on the archived branch
`pending/r36-preserved-20261117` at commit `4cdf418`.

## Accepted isolated checkpoint

Candidate `4b19f55` combines Luna's normalized header and narrower target changes
with Terra's search changes. Seed removals are replayed centrally from the fixed
base. Three registered regression families contain 23 compositional cases,
including independently mutated reference operands; all 23 pass together.

The candidate removes six seeded rules and is now an accepted isolated checkpoint.
The full audit passed all 32,755 occurrences, preserved all 33,730 native readings
and every baseline-successful print gate, with no failures. All eight repository
gates passed, including 996 library tests (one ignored) and 802 family cases.
The full audit took 18m 51s. The validated mechanical and authoring changes
were subsequently integrated into main as `093b41a` and `2d83443` respectively,
retiring 13 seeded rules in total. These commits record integration into main;
they do not claim a release or completion of the remaining campaign.

The rescan reports 9,986 seeded rules, of which 9,979 are still needed and seven
are unused. Two are newly exposed mechanical candidates:
`t_search_your_library_for_up_to_value_4` and
`t_you_get_an_emblem_with_affected_static_changes_3`. The other five retain prior
review findings. Accepted checkpoints should feed new unused candidates back to
the mechanical lane and publish their shared capabilities for later authoring jobs.

The preceding seven-rule mechanical checkpoint already passed all 32,755 allowed
occurrences, preserved all 33,730 native readings, and passed every repository
gate. Its rescan found no new mechanical candidates.

## Accounting and limits

Costs use the [public Standard API rates](https://developers.openai.com/api/docs/pricing),
checked November 16, 2026. Assigned-thread usage was collected with the existing
Vizier estimator and independently matched Cantrip's card-lane estimator for all
six workers. Cached input is discounted and reasoning is a subset of output.
These estimates exclude supervisor work, host compute and service-tier/regional
adjustments; they are not subscription charges.

Frozen descriptors, source/helper hashes, all failed attempts, scoped usage,
expanded reports and coordinator normalization provenance are retained outside
tracked source. No full corpus, raw prompts, session transcripts or machine paths are
committed. The trial demonstrates why shared grammar work needs broader checks
and coordinator integration even when every worker reports a passing local cohort.
