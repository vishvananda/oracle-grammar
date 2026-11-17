# Shared grammar retirement lane

Status: the [paired authoring pilot](retirement-authoring-pilot-20261116.md) and
the seven-rule mechanical checkpoint passed their integration gates. The rolling
campaign now supports mechanical deletion, shared grammar authoring and automatic
integration after validation. Each attempt uses a private checkout, fixed task
descriptor, frozen helpers and independent coordinator evaluation. The deletion
lane does not authorize shared grammar or Rust edits; the authoring lane permits
only its declared grammar and fixture changes.

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

The descriptor-bound `--remove-assigned` command reconstructs seeded files from
the immutable baseline and removes the assigned rules with the existing removal
implementation. It saves private backups and preserves authored modules. Workers
should use it instead of editing large seeded hook lists and semantic comments.
Coordinator evaluations run in separate processes so concurrent checks cannot
share another task's lease descriptors. The rolling scheduler starts a pair's
second model as soon as its first finishes; it retains the chosen first-model
ordering without waiting for unrelated tasks.

Workers declare cross-module registration in `.cache/author-hooks.json`, for
example `[{"root":"ability","production":"shared_trigger_name"}]`. The runner
admits only bounded, typed entries for ordinary `ability`, `line` or
`event_clause` lists and applies them to its diagnostic copy. The authored source
keeps its private write set. The tested grammar hash includes these registrations.
Seeded files must match the canonical assigned-rule removal, ignoring blank lines;
adding handwritten roots to a mined dispatcher is rejected even when a
compositional-prefix check would otherwise pass.

Every candidate remains provisional until coordinator integration gates pass.
Workers never merge source or run a corpus per job; the coordinator stages a
combined candidate and audits it once. Acceptance is not inferred from agent prose.
Mechanical staging first verifies that the complete evaluated grammar exactly
matches its frozen grammar after canonical assigned removals. Such removal can
also delete bare seeded alternatives from ordinary dispatchers. Staging then
repeats the removals on the current accepted prefix, preserving newer shared
productions and alternatives instead of merging old dispatcher text. Mechanical
hooks, extra or missing files, wording changes and unrelated semantic or compiled
input edits remain forbidden; all existing validation gates still run.
The results should
determine which families Luna can handle, which go directly to a larger model,
and whether to increase authoring concurrency toward twelve.

## Rolling batch checks

The rolling dispatcher accepts `stage_group_size` from one to four, with a
default of one. A larger group merges its proposals in order into a disposable
grammar and checks the complete union against every baseline, previously admitted
and newly authored family, plus the configured native regression controls. Only
a passing union replaces the accepted prefix. A failed group retains its proof,
discards the trial and retries its members individually against that prefix.
Separate passing subsets never prove that their union passes. Manifests retain
the ordered job dispositions, grammar and fixture hashes, group proofs and
incremental staging progress.

An operator can stage an explicitly selected cohort in one trial with
`retire_dispatch.py batch RUN --jobs-file FILE --combine-selected`. This requires
a nonempty list of at most 96 distinct provisional jobs; it does not expand the
ordinary configuration limit of four. The union runs every baseline, previously
admitted and new family, plus the native and library controls, before promotion.
A failed union is discarded and its members follow the existing individual
fallback. The manifest records both the option and actual group size. This is
useful for an isolated recovery cohort whose union has already been investigated;
it does not waive fresh staging or full integration validation.

Family preparation shares a stage-local cache of at most two frozen gold blobs,
keyed by SHA-256. Each access checks the expected hash and hashes the actual
file, including on cache hits. The index retains every matching card record and
does not collapse different cards sharing a face name. Operand edits use deep
copies, and a new stage starts with an empty cache.

Two retrospective replays exercised this path on identical immutable proposals,
using quick diagnostic helpers and six shared CPU tokens. Four selected passing
proposals took 365.37 seconds individually and 164.65 seconds together. Eight
proposals containing two known family failures took 672.06 seconds individually
and 693.55 seconds with groups of four and fallback. Both replays produced
identical final grammar, fixtures and dispositions. These are single selected
pairs under concurrent load, not general throughput estimates. Monitor actual
group success and fallback rates before extrapolating the savings.

Full candidate validation still requires the eight repository gates and the
complete explicitly allowed population, including every native reading and each
baseline print success. Shared Cargo targets can reuse a test executable with a
different compiled checkout path. The coordinator gate driver therefore binds
the native gates and receipt identities to the candidate grammar and clears
inherited grammar overrides for the fast gates. It reuses the candidate's own
commands, compiler settings, gate timing and receipt implementation. Gate
processes inherit the coordinator's lease descriptors. Old checkpoint release
tests remain independent of a parent's grammar override.

The optional boolean `full_parse_screen` adds a rejection screen after the
repository gates and before the full print audit. It scores the complete frozen
allowed population without rendering, then validates every native reading against
the checkpoint baseline. The screen checks complete population coverage, source
and native counters, reading identities and command receipts. A validator exit
of zero alone is insufficient: its preservation report can still list added
readings. Invalid or incomplete evidence stops validation as an operational
failure. Candidate, baseline and frozen input hashes are checked before and after
the screen. A semantic rejection is recorded separately as `parse_screen`;
passing still requires every existing full print and native check. The screen
does not supply a full-validation result.

An optional `candidate_validation_helpers` setting names a hash-bound optimized
helper build. It remains separate from the parent checkpoint's helpers while
staging. Promotion requires a private verified helper snapshot, the full candidate
audit and all registered families under the optimized helpers. A failed candidate
cannot replace the checkpoint's helper identity.

A batch rejected only by a bounded set of library tests can return other
independently passing proposals to validation after explicit isolation. The proof
must reproduce the full frozen candidate, replay the remaining proposals in order
with every identified culprit excluded, and show the exact failed tests passing
on both the baseline and the remainder. The same hashed test executable runs all
controls against explicitly selected grammars. Atomic multi-job readmission
verifies the shared replay once, checks every proposal's original evidence and
ownership, and rechecks artifacts before changing any queue rows. It preserves
the rejection history and requires fresh combined staging and full validation.

Historical batches whose native validator exited one can retain an operational
exception in their original validation record even when the artifacts establish
a complete preservation rejection. The native exit-one readmission API handles
only that narrow case. It verifies the exact command and receipts, original
repository gates, frozen population and baseline, and a hash-bound diagnostic
supplement without rewriting the rejection. It recomputes the historical native
comparison once and independently replays the retained proposals once. Baseline,
full-candidate and remainder controls must use the same explicit population and
cover every originally reported failing face. All selected jobs are readmitted
atomically under their leases, with dead-process, artifact, checkpoint and row
checks repeated before the transaction. Readmission is allowed once per job and
failed batch; the excluded proposals stay parked. These jobs still need fresh
staging and the complete integration audit.

An attempt with no usable model response is an operational gap, not a semantic
finding. A verified original timeout owning four to six rules can be split once
into two children of at most three rules. Recovery requires the archived attempt,
an empty final response, exact timeout and process evidence, a released job lease,
the current checkpoint proof and unchanged rule ownership. The parent and its
evidence remain available; children cannot recursively use this recovery. Smaller
timeouts require separate review. A coordinator triage supplement may identify
per-rule parse dependencies or inspect saved candidate diagnostics, but must keep
the unavailable model disposition and any untested gates explicit. An earlier
passing diagnostic cannot validate subsequent edits to the archived candidate.

The read-only review export accepts paired `--coordinator-manifest ABSOLUTE_JSON`
and `--coordinator-manifest-sha256 SHA256` options. The explicitly selected
manifest pins reviewed indices, their evidence and bounded canonical evidence
roots; the campaign cache is always allowed. The exporter verifies each original
job, attempt, rule assignment and nested evidence before attaching a separate
`coordinator_triage` field. It retains unavailable model outcomes and unclassified
assessments, lists untested gates, and cannot accept a retirement. Current
ownership and verified inherited review origins come from the same database
snapshot. Stale or reassigned findings remain unmatched with an explicit reason.
Without these options, the existing export is unchanged.

`tools/retire_timeout_operator.py --run RUN --plan ABSOLUTE_JSON` inventories
finished attempts and rehearses eligible splits against a SQLite backup. Repeated
`--job ID` options restrict the selection. Planning never changes the live queue;
the plan records parked reasons, exact artifact hashes, verifier code identities,
the accepted checkpoint and the complete expected rule inventory. Apply requires
the same arguments plus `--apply-plan-sha256 SHA256 --receipt ABSOLUTE_JSON` and
reruns the existing recovery API checks. Each successful split commits separately;
a later error stops the operation and its atomic, durable receipt retains completed
jobs. Child claims or reclassification after a split do not invalidate its immutable
creation evidence. This operator does not retry smaller timeouts, infer semantic
success or accept a retirement.

`tools/retire_auth_recovery.py` plans and rehearses retries for explicitly selected
terminal HTTP 401 attempts or the exact CLI token-refresh error caused by signing
out or changing accounts. Generic authentication, quota and semantic failures do
not qualify. It requires an empty final response, no completed
model disposition, a non-timeout failure receipt, matching descriptor and archive
evidence, an absent recorded PID and the current job lease. Apply pins the plan
and verifier hashes, rechecks the current attempt inside the queue transaction,
and records a `retry-auth` event before a fresh claim. Original attempts, results
and archives remain available; the tool does not accept rules. A live controller
may continue other jobs because the retry holds the affected job's kernel lease.
Each job commits separately, with durable progress receipts; a failure stops the
operation and must be inspected before any further recovery. Verified archived
candidate changes require separate restoration review before reuse.

Per-worker native helpers use verified independent copies. Copy-on-write support
can share their storage without sharing a writable inode; a private helper edit
must not change either the checkpoint helper or another worker's helper.

After a passing family gate, author diagnostics with more than 128 closed face
occurrences first score the transitive target-face closure without rendering.
This screen can reject name, text, recognition or reading-count regressions; it
cannot accept a proposal. Surviving proposals still run the complete original
cohort, native reading validation, all four print gates and operand-edit checks.
Initial cohort selection retains its 500-name bound; an already closed cohort
can exceed that name count while remaining within 1,024 face occurrences.
Audited-unused mechanical tasks that exceed either bound use at most eight
complete chunks. Each chunk preserves transitive shared-record components,
starts from at most 500 names and contains at most 1,024 occurrences. No target
or companion is sampled away; an oversized component or excess chunk count
fails closed. The descriptor binds the complete plan, and the worker recomputes
it from the full frozen inputs before running the unchanged native and four
print checks on every chunk. Reports distinguish completed, unrun and incomplete
chunks and require complete passing coverage. Ordinary tasks keep their existing
descriptor and diagnostic path.
For eligible new tasks, preparation also saves a bounded primary-cohort packet
bound to the task descriptor, support record, and frozen input manifest. It
retains the original occurrence addresses, gold records, and Oracle companions.
Diagnostics verify that packet and use it for the early rejection screen without
loading the entire population. A missing or altered promised packet fails closed;
older tasks and oversized packets use the existing full-input path. Passing the
screen still loads the full data and runs every original acceptance check.

Optional hash-bound library regression controls run a small list of exact tests
against the baseline and each trial grammar before promotion. Their executable
must match the checkpoint's compiled source and compiler identity. Each test must
actually run once and pass; a zero-test result cannot pass. Failed groups fall
back to individual checks. These controls supplement the normal complete suite
and full population audit.

The standalone batch driver can hold the campaign batch lease while model workers
continue authoring under their separate controller lease. It runs the existing
batch pipeline sequentially and records its own status. Expected semantic
rejections remain parked and later ready work continues. Configuration, integrity
or missing-evidence failures stop the driver for review. A drain takes effect
after the current batch finishes. This permits validation-tool upgrades without
restarting the authoring pool.

With the optional boolean `external_batch_driver` enabled, the author controller
does not launch its legacy batch subprocess. The separate driver owns staging;
author claims, draining and completion checks stay unchanged. This prevents the
author controller from competing with a replacement driver during handoffs.
The option defaults to false and rejects nonboolean values.

The optional boolean `recovered_cohorts_first` lets that driver prioritize
readmitted proposals. It uses the latest readmission event for each currently
provisional job only when the event names its current attempt. Jobs are grouped
by original failed manifest and readmission checkpoint; the oldest available
group is selected separately, up to 96 jobs. It uses the guarded combined trial
only while its readmission checkpoint is still current. After the checkpoint
advances, the cohort stays separate but uses the configured groups of at most
four: a passing union at an older checkpoint does not establish a passing rebase.
Fresh proposals are not added to that cohort. Stale attempt events are ignored;
malformed matching identities stop the driver. This is a routing hint, not an
acceptance certificate, and a changed checkpoint still requires fresh staging
and full validation. With no eligible cohort, ordinary batch size and timing
rules apply. The option defaults to false.

The optional boolean `mechanical_cohorts_first` prioritizes ready mechanical
removals. When their count reaches the ordinary batch threshold, the driver
selects up to 96 mechanical proposals as one guarded combined trial. Smaller
mechanical queues keep recovered-cohort precedence; otherwise they lead an
ordinary-sized selection filled with FIFO author proposals. The normal timing,
readiness and flush conditions still apply, so a small mechanical queue does
not force a separate early audit. Driver status records the selected lane.
The option defaults to false and changes only routing: all staging, native,
library, family and full integration checks remain required.

When an explicitly combined selection fails and exceeds the configured ordinary
group size, staging tries ordered subgroups of that size before individual
fallback. Each subgroup runs every existing guard against the accepted prefix;
only a failing subgroup is retried individually. The manifest retains the whole
union and subgroup proofs and records the fallback size. Equal-sized trials are
not repeated, and uncertain promotion still fails the entire batch. This reduces
repeated validation when one proposal conflicts with an otherwise usable cohort;
it does not change the final integration checks.
