# Shared grammar retirement lane

Status: a bounded paired authoring pilot is running. The mechanical Luna lane
has produced an accepted isolated seven-rule checkpoint. The deletion runner
does not authorize grammar or Rust edits; the pilot uses separate private task
descriptors, checkouts, frozen helpers and coordinator evaluations. Broad
authoring scheduling and automatic integration remain outside this pilot.

## Job and checkpoint contracts

An authoring job owns a related family of seeded rules, rather than one card or
one arbitrary batch of names. Its immutable descriptor records:

- Accepted source commit, grammar tree, compiled-source identity and helper hashes.
- Explicit allowed input addresses and hashes, baseline readings and four print gates.
- Target seeded rules, proposed shared productions, dependency job IDs and input
  checkpoint. A rule can belong to only one active family at that checkpoint.
- Expected files, production symbols and semantic contracts affected; a narrow
  write set with a reason for each change.
- Allowed regression fixtures and edited-operand, ambiguity and refusal controls.
- Model, effort, task limits, attempt identity and stop/escalation conditions.

Inventory includes every remaining seeded rule. The initial accepted snapshot
has 9,999: 14 parse-unused candidates and 9,985 needed rules. Needed does not mean
irreducible; it means deletion alone has no accepted proof. Count retired rules
only after integration, never after triage or a worker's claim of success.

Group jobs by missing phrase capability and semantic contract. Name prefixes
and shared files can suggest groups but cannot prove independence. Prioritize
small, well-understood syntax compositions before contract or binding changes.
Keep a separate queue for printing repairs exposed by mechanical deletions.

## Isolation and scheduling

Each attempt receives a private checkout of its exact accepted checkpoint.
Use the existing Btrfs CoW machinery for complete authoring checkouts. Do not
reuse another worker's writable checkout or target directory. A kernel lease
owns the family; attempts retain unique paths and immutable receipts after exit.
Unknown child-process state blocks reassignment until the controller verifies
the old process and its inherited leases have ended.

Use up to twelve Luna Max authoring slots, initially with two diagnostic helper
threads each. This concurrency is a proposed limit: the deletion-only resource
probe does not establish authoring or compilation capacity. Start the paired
pilot at three concurrent workers. A single coordinator build lane handles any
compiled-input changes; twelve independent Cargo builds are prohibited.

Schedule jobs whose declared production/contract dependencies do not overlap.
One job owns a shared foundational change. Dependent jobs wait for its accepted
checkpoint, then receive that implementation. Avoid concurrent edits to global
hook lists by having the coordinator add declared hook entries during staging.
File separation alone does not establish semantic independence.

If a job discovers a dependency or needs files outside its declared write set,
it records the discovery and returns to scheduling. It does not acquire an
unbounded collection of locks or expand its authority by editing the descriptor.
Unrelated jobs can continue while a shared dependency is under review.

## Worker responsibilities

The author may add reusable phrase productions with one shared `@sem`, remove
the assigned seeded rules, and add focused tests. Renaming or adopting a whole
seeded production is not retirement. New syntax must express a shared capability
and preserve original Oracle wording and every supported semantic reading.

Syntax-only jobs reuse the checkpoint's frozen helpers. A controlled diagnostic
command selects the job's allowed cohort, including companions and duplicate
occurrences, and compares all native readings and four print gates. Its cohort
includes rule users, explicitly identified shared-production users and controls;
it remains a diagnostic heuristic, not a replacement for full validation.

Require actual current-operand printing tests: change a quantity, recipient,
filter or other relevant operand and verify semantic reparse. Add negative and
ambiguous cases when the new production could overmatch. Exact wording, semantic
roundtrip and reference agreement remain independent results. No source-text
rewrite, opaque success, first-reading shortcut or expected-IR edit is allowed.

A need for Rust, lexical, IR or binding changes exits the syntax-only lane.
The coordinator admits a separate contract job and performs one source-matched
build after grouped changes. Existing reference/model modification rules remain
in force; a worker cannot repair a discrepancy by changing the baseline.

## Reusing earlier work

Maintain an append-only findings index keyed by source/input hashes, rule family
and production contract. Each entry links to the attempt, patch, failed gates,
tests and evidence digests. Pending findings can save diagnosis time but cannot
grant acceptance. Keep a producer's pending patch out of downstream source.

After integration gates pass, publish an immutable checkpoint and a short recipe:
the shared production's contract, intended callers, exclusions and regression
tests. Later workers receive the checkpoint plus relevant recipes, rather than
full transcripts. Match compiled-source hashes before reusing binaries. Match
grammar, corpus, baseline, command and budget hashes before reusing check results.

A checkpoint change does not silently rebase active jobs. Their results retain
their old base identity. The coordinator stages them against the new checkpoint,
resolves conflicts explicitly, and repeats affected checks. A clean textual
application is not semantic acceptance. Deduplicate failed retries by identical
input/patch hashes; a new attempt needs a changed hypothesis or dependency.

## Integration and escalation

Workers return patch, changed-file inventory, test evidence, dependency findings
and a disposition. Only the coordinator stages patches, with a compare-and-swap
check on the accepted checkpoint. Stage dependency order and small related
batches. Run affected families and required repository gates in that checkout,
then one immutable full allowed-population parse/native/print audit for the
combined batch. Preserve all readings, source identities and all four print
gates. Failures block promotion and identify a repair or batch-bisection task.

Publish the accepted commit, grammar/helper/input hashes and complete evidence
only after every gate passes. Rescan once per accepted batch for newly unused
rules; feed those into the mechanical lane. This makes shared phrase work
available to later workers and can retire additional rules without new authors.
No authoring worker applies changes to the main checkout or publishes a release.

Escalate to a larger model for identity/scope proofs, ambiguous meanings, changed
readings, compiled contracts, uncertain dependencies, print inverse repairs that
need binding changes, or a bounded unsuccessful Luna attempt. Include the exact
failure, minimal allowed reproducer, baseline/patch identities and hypotheses
already tested. A tool/path/process error is an operational repair, not evidence
that semantics require a larger model. The current mechanical review queue is
input to this lane; it is not automatically resolved by running a larger model.

## Paired Luna/Terra authoring trial

Before a broad authoring campaign, implement and test the descriptor, checkout,
bounded-check, evidence, escalation and checkpoint protocols above. Then run
Luna Max and Terra High on the same frozen independent task bundles. Include:

1. A small reusable syntax composition that currently needs seeded rules.
2. A phrase family with independent edited operands and refusal controls.
3. A mechanical deletion whose printing dependency needs a shared grammar repair.

Select tasks from the explicit allowed inventory, without inspecting held-out
cards. Give both models identical source, fixtures, baseline, tools and budgets.
Keep their findings and patches separate until paired attempts finish. Run the
second model's tasks from the original checkpoint, not the first model's output.
Alternate model ordering across bundles and repeat where useful to distinguish
task variance from a model effect. Record time to first candidate and time to
fully validated result, helper/build time separately, scoped token usage and
API-equivalent cost using the existing card-lane estimator.

Judge quality with deterministic checks and coordinator review: valid shared
meaning, readings retained, four print gates retained, edited-operand behavior,
refusals, legitimate retirement count, out-of-scope edits, repair iterations
and correct escalation. Do not require byte-identical implementations. Report
incomplete/escalated tasks rather than dropping them from medians. The earlier
mechanical pilot does not establish authoring quality or speed.

The paired pilot uses `tools/retire_author.py` for bounded checks and
`tools/retire_author_pool.py` for a retained three-slot model launcher. Three
tasks cover mixed trigger headers, AnyNumber search choices and optional
creature/planeswalker graveyard targets. Each pair starts from the same accepted
checkpoint; first-model order alternates across tasks. Workers have eight
diagnostic attempts and a 25-minute task budget, with two helper threads each.
They author focused families and current-operand edit fixtures. The runner
applies those primitive edits to immutable reference IR before score/print/reparse,
and the coordinator independently snapshots and rechecks final candidates.

Every candidate remains provisional. The pilot does not merge author patches,
run a corpus per worker, or infer acceptance from agent prose. Its results should
determine which families Luna can handle, which go directly to a larger model,
and whether to increase authoring concurrency toward twelve.
