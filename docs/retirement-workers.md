# Retirement campaigns

`tools/retire_campaign.py` prepares a durable first-pass campaign over every
remaining seeded rule. Its inventory assigns each rule to exactly one family
job. A rule is accepted only after its attempt has an explicit per-rule pass
and the coordinator accepts the combined checkpoint. Unsupported, failed and
needs-review rules stay visible with their reasons; a successful helper exit is
not a semantic disposition.

The rolling pool has up to twelve Terra High authoring workers. Each works in a
private checkout from its claimed checkpoint, diagnoses every assigned rule and
may perform at most three diagnostic checks within a 900-second budget. A rule
shown unused by the current audit can instead take the deterministic mechanical
lane; uncertain cohorts, retained users and failed checks remain for review.
Workers do not write shared grammar, model or reference files. Their descriptor
bounds the writable files; ordinary root hooks are registered explicitly, and
seeded removals use `mined.remove` rather than manual hook edits.
Workers start with the inferred module hints and may compose changes across the
existing runtime syntax modules. Direct changes to the protected `ability`,
`line` and `event_clause` dispatchers are rejected; typed hook requests supply
their additions. Lexical, model and reference files remain outside the write set.

The SQLite queue records the exact inventory, atomic claims, immutable attempt
results and an append-only audit. Running work has no time-based expiry: an
unknown or orphaned child is not silently stolen or retried. Recovery requires
verified evidence. Each attempt retains its original checkpoint even as later
claims use a newer one. After a terminal receipt, the private checkout can be
verified and archived before its worktree is removed; retain the evaluation,
report, patch and provenance needed for review.

The worker normally archives its checkout after finishing. Schedule
`python3 tools/retire_storage.py CAMPAIGN --apply` every 15 minutes for a
long-running campaign to reconcile checkouts left behind by interrupted
workers. Run without `--apply` to preview eligible checkouts. Cleanup takes
the same per-job lease as the worker and checks the terminal receipt and
recorded process before verifying the archive. It can reuse a verified archive
when removal was interrupted. The latest applied pass records its removals and
retention reasons in `CAMPAIGN/.cache/storage/worktree-cleanup.json`. Attempt
reports, frozen inputs and audit evidence remain available for replay.

When a reviewed archive comes from an accepted ancestor, the controller can
reuse the existing production merge to place the draft on the worker's current
checkpoint. It reapplies only the assigned seeded removals. A merge conflict
leaves the worker with a clean checkout and the previous review notes. Every
restored draft still goes through fresh author, coordinator and integration checks.

The coordinator stages disjoint proposals in a private integration workspace.
It merges ordinary productions conservatively, replays the union of assigned
seeded removals, applies approved hooks and registers family fixtures. Conflicts
are parked with their reason. The first batch targets 12 proposals, later batches
48, with a hard staging bound of 96. A candidate commit is provisional until
its family checks, full audit and repository gates pass. A failed combined audit
is evidence about the batch; it does not identify an individual culprit.
Repository gates run before the expensive whole-population score. Trial grammar
must retain the families from earlier included proposals; a failing new proposal
is parked without discarding those earlier proposals.

When the external batch driver has `fresh_union_trial` enabled, a fresh
selection first tries its combined grammar in one guarded stage. If that trial
fails, staging tries the configured subgroups and checks unsuccessful subgroups
individually. When the failed families belong only to new proposals and those
proposals fit into one subgroup, staging first gives the remaining proposals a
guarded trial. If it passes, each withheld proposal gets a separate trial.
Mechanical priority and recovery routing still govern selection. The same
repository and full allowed-population checks gate acceptance.

Worker-authored cases always run compositionally. Coordinator-generated
whole-card operand cases may explicitly retain unrelated seeded clauses; these
cases still require their independently mutated expected IR and verified prints.
The family checker reports compositional and seeded case counts separately.
It accepts at most 4,096 files, with aggregate limits of 1 GiB of actual input
and 262,144 cases, retaining the per-file limits of 4 MiB and 4,096 cases.

A completed native audit can be recovered after a diagnostic-only runner repair
only when Git independently proves identical grammar and native implementation,
excluding the diagnostic CLI and its tests, and all repository gates pass again.
The original failure remains in the audit trail. Readmission of other parked
proposals only restores provisional status; their combined grammar is rechecked.
A parse-screen isolation can cite the accepted ancestor where the batch
originally failed. The focused controls must reproduce every added reading
or every refused occurrence and its missing native readings. The queue
verifies the original proof and restores only the still-current rejected
attempts. Staging checks the resulting union on the latest checkpoint before
full validation and publication. An archived full-check failure can use the
same recovery path if all prior readings remain and focused controls reproduce
every added reading and print failure.

One full audit runs at a time, reserving 16 tokens by default from a shared
32-token CPU pool; a 24-thread option is also supported. Worker helpers reserve two
tokens through verified wrappers; batch trial family checks reserve six. A pending large allocation temporarily stops
new small allocations; after it acquires its tokens, workers may use the remaining
capacity. Checkpoint rule scans use the same reservation mechanism. Ordered
readmission replay uses one CPU slot with waiting priority so a busy worker pool
cannot starve recovery. Active audits and worker checks retain their leases.
Private batch trials can set `background_cpu` in their checkpoint copy to wait
for an idle bundle while leaving available tokens open to foreground checks.
For the current explicitly allowed population, that means all 32,755
occurrences: preserve every native reading and every baseline-successful
verified, exact keyed, default and exact-default print gate. The population and
its immutable gold, Oracle, baseline and helper hashes belong to the checkpoint;
the count is not a claim about a different corpus. Eight repository gates run
against the candidate. On success the coordinator freezes the candidate, rescans
for newly mechanical rules and makes it the base for subsequent claims. The
stable integration workspace and shared Cargo target retain build caches, while
checkpoint inputs, candidate commits and audit evidence remain separate.

Later accepted grammar can also make a parked rule unused. Reclassification can
then create a fresh mechanical job from the verified checkpoint scan. The
original attempt and receipts stay immutable, and other rules in that bundle
stay parked with their original review evidence. This creates no model attempt
or acceptance; the mechanical job still needs fresh checks and full batch
validation.

The optional `priority_policy` value `frontier-shape-v1` ranks pending author
jobs once per accepted checkpoint. It uses source complexity, the audit's selected
derivation and matching semantic shapes as scheduling hints. Mechanical jobs
stay first; every fourth author claim uses FIFO. Stale checkpoint ranks are
ignored, and publication never changes a job's payload, ownership or acceptance.

An optional hash-bound `native_regression_controls` profile lists allowed face
names. Staging verifies the base against that closed population, then checks each
family-passing trial before admitting it. This catches known cross-family
reading and default-print regressions early; the full audit still runs.
Repeated staging on the same accepted grammar, input snapshot, helpers and
control profile can reuse the passing baseline report and its verified files.
Each trial runs its own controls against its candidate grammar.

Task cohorts close transitively over shared face names in frozen gold records,
including every duplicate occurrence and companion face. This expansion does not
change the assigned target faces. Diagnostics refuse closures above 1,024 face
occurrences instead of truncating them. Old population-mismatch failures can be
retried only with artifact-backed proof, a terminal controller, an absent worker
process and a released job lease; the original attempt remains in the history.

Before admitting each proposal to a batch, staging checks every registered
baseline family, earlier admitted families and the new family against the trial
grammar in one helper invocation. A failing proposal stays out of the union;
later proposals can continue. Baseline fixture hashes and explicit trial module
overrides keep this check tied to the checkpoint. Repository gates and the full
allowed-population audit still decide acceptance.

An independently checked author repair can return its original assignment to
the Terra queue once. The retry verifies the report against its accepted
checkpoint, the assigned rules, the allowed cohort, the family check and the
edited operands. The worker receives the coordinator's reproduction note and
authors a fresh proposal on the latest checkpoint. An independent repair with
retained grammar, family and operand fixtures can also seed the private
checkout. The preparer checks the artifact hashes, rebases the exact assignment
and uses the ordinary archive or clean checkout if the repair cannot be reused.
The worker's preview and independent validation run on the new checkout.
The previous attempt remains in the audit history. When a model times out before submitting its dispositions,
a passing coordinator report can supply this evidence if it belongs to the
protected terminal receipt for that attempt.

Author diagnostics stop when the compositional family fails, explicitly marking
cohort and edited-operand checks as skipped. Once the family passes, every normal
cohort, native and edited-operand check runs. Edited-operand diagnostics also
check the generated contextual family cases before reporting a pass. A skipped
check cannot establish success.

Campaign workers use `--preview` before returning their per-rule dispositions.
It checks the compositional family, native readings and printing for the
assigned target faces and their companions. The report identifies this scope
as `author-target-preview` and records the checks deferred to the coordinator.
The independent coordinator checks the complete assigned cohort and edited
operands. The full diagnostic is also available directly and remains the
default for paired pilot workers.

If a native diagnostic outlives its model process, the coordinator waits for
that task's diagnostic lease before reserving CPU for independent evaluation.
The retained diagnostic can finish before the worktree is archived.

Configured usage collection records attempt-scoped token counts and estimates
Standard API-equivalent cost using a pinned estimator and explicit pricing.
Unavailable or partial usage is reported as such, not as zero cost; estimates
are not subscription charges. Preparation and status use the campaign CLI:

```sh
python3 tools/retire_campaign.py prepare CAMPAIGN --source CHECKOUT --helpers HELPERS \
  --gold GOLD --oracle ORACLE --allowed ALLOWED --baseline BASELINE \
  --main MAIN_CHECKOUT --target CARGO_TARGET
python3 tools/retire_campaign.py status CAMPAIGN
python3 tools/retire_observe.py CAMPAIGN
```

Completed paid attempts retain usage even when their final review is invalid.
For older attempts missing usage, `python3 tools/retire_usage_refresh.py CAMPAIGN`
can collect only the assigned thread's usage into separate, hash-bound receipts.
It does not edit queue results. The observer uses a matching receipt only when
original usage is absent, so an attempt is counted once.

The read-only observer separates model time from claim-to-finish slot time and
reports every retained attempt, including verified infrastructure retries.
Provisional candidates, accepted rules and parked reviews remain separate.
To upgrade the worker controller while an external batch driver continues
validating, write a reason to `CAMPAIGN/controller-drain.json`. The controller
finishes active attempts and stops claiming work. Verify that its process is
terminal before replacing the runner or removing the drain request. Use
`CAMPAIGN/drain.json` to drain both processes.

An immutable version can instead live at `CAMPAIGN/runners/VERSION`, with its
complete file-hash map at `CAMPAIGN/runners/VERSION.json`. Launch that version's
`retire_campaign.py run CAMPAIGN --runner CAMPAIGN/runners/VERSION` after the
previous worker controller and its attempts finish. Its controller and imported
modules must match the selected version, and its frozen tooling must match the
checkpoint. An audit already using the older runner can finish independently;
neither snapshot is changed. The shared queue, controller lock and CPU leases
still enforce campaign ownership and concurrency limits.

For a curated retry, a frozen dispatch runner accepts
`batch CAMPAIGN --jobs-file /absolute/selected-jobs.json --wait`. The file is a
nonempty ordered JSON list of at most 96 unique, currently provisional job IDs.
Other provisional work stays queued. The optional wait acquires the same batch
lock as automatic batches; every selected proposal still receives normal
staging, repository gates and full-population validation.

This describes the campaign workflow, not evidence that a campaign has started
or completed. See [the authoring pilot](retirement-authoring-pilot-20261116.md)
for measured results and their population.

## Legacy deletion-only worker

The following `tools/retire.py` lane predates the rolling authoring campaign.
Its frozen jobs and commands remain useful for a bounded deletion-only audit;
its pool, claim barrier and checkpoint procedure are distinct from the campaign
queue above.

Use `tools/retire.py` to delegate mechanical candidate removal to Luna workers.
The coordinator owns one frozen campaign; workers own jobs, never source files.
No command builds Rust, edits a checkout, commits, changes reference/model data,
or publishes. A worker pass is **provisional**, not permission to merge.

## Prepare once

Start from a clean reviewed source commit and source-matched prebuilt helpers.
Use only an explicitly allowed population. Gold and Oracle inputs must contain
exactly that population. Baseline parse rows must cover every face and be equal;
printing overlays must cover every face, oldest first, newest last. A source
change needs a new campaign; a reference change needs newly prepared inputs.
Occurrence order and original record/face addresses are preserved: distinct
source records may have the same name and text. Print overlays must contain
every occurrence of each selected name, rather than silently choosing one.

```sh
python3 tools/retire.py freeze .cache/retirement/run-01 \
  --source /path/to/clean/checkout --helpers /path/to/frozen/helpers \
  --gold /path/to/prepared/gold.jsonl --oracle /path/to/prepared/oracle.json \
  --allowed /path/to/allowed-slice.json --parse /path/to/accepted/parse.jsonl \
  --prints /path/to/oldest-print-rows.jsonl /path/to/newer-print-overlay.jsonl \
  --slots 2 --threads 2
python3 .cache/retirement/run-01/frozen/runner/retire.py scan .cache/retirement/run-01
```

Freeze copies grammar, checked helper executables, workflow implementation and inputs once, records
checksums and the compiled-source identity, and publishes the complete bundle
atomically. Use the returned frozen runner path for the remaining commands.
Its imports do not create bytecode inside the snapshot. The coordinator may
improve checkout tooling while existing campaigns keep their original runner.
Scan uses that bundle for one whole-population rule audit. Only
its unused seeded rules become jobs (`scan --rules NAME...` can narrow the queue).
An unused rule preserves a matching
reading; that alone proves neither all-reading preservation nor printing.

## Give a Luna worker this task

> Read AGENTS.md and docs/retirement-workers.md. Run
> `python3 /absolute/campaign/path/frozen/runner/retire.py worker /absolute/campaign/path --name luna-01 --limit 4`.
> Run this directly; the CLI records its own start/end and elapsed times under
> `workers/`. Retain a yielded exec handle and wait on that same handle. Do not
> wrap a long command in another timing process or restart on observation timeout.
> Use the frozen runner's `report` command to inspect your assigned rules and
> follow the explicit artifact paths instead of guessing attempt directories.
> Inspect the returned job reports and their logs. Do not edit grammar, Rust,
> fixtures, baselines, snapshot files or job results manually. Do not run Cargo,
> whole-corpus checks, Git commits, merges or releases. A provisional pass needs
> coordinator validation. Report needs-review/failed jobs with the failed gate
> and artifact path; do not repair semantics, widen equality, suppress a reading,
> weaken a print gate or rename/adopt a seeded rule to make the job pass.

Workers invoke the existing `mined.remove` implementation against private
grammar copies. Each job checks a bounded diagnostic cohort, including exact
keyed printing, verified default printing, exact default wording and native reading preservation.
Simple literal subsequences select this cohort; alternative/unknown surfaces,
empty or large cohorts, retained users and failed gates require review. The
cohort is a triage heuristic, not a complete dependency analysis.

Kernel-held locks prevent double claims. They are held through subprocess
completion; crashed processes release them. There are no expiring leases that
allow a late worker to overwrite a newer attempt. A recovered attempt keeps
the interrupted attempt directory. Helpers inherit the leases, and unique
attempt paths also keep orphaned outputs separate. Atomic JSON writes expose complete state.
Slots cap concurrent workers and total helper threads. Use one active campaign
per coordinator CPU budget; separate campaigns have separate slot pools.

```sh
python3 .cache/retirement/run-01/frozen/runner/retire.py status .cache/retirement/run-01
python3 .cache/retirement/run-01/frozen/runner/retire.py report .cache/retirement/run-01 --rules RULE_A RULE_B
```

## Coordinate once per batch

```sh
python3 .cache/retirement/run-01/frozen/runner/retire.py assemble .cache/retirement/run-01
python3 .cache/retirement/run-01/frozen/runner/retire.py validate .cache/retirement/run-01
```

Assembly reserves the coordinator, rejects live jobs and changed candidates,
and replays the union of passed **rule names** onto one fresh grammar copy.
It never merges worker patches: independent workers can touch the same mined
file safely. A claim barrier closes the worker phase before assembly, so new
workers cannot race collection or oversubscribe combined validation. It emits
`combined/retirement.patch` for review.

Validation parses and renders the **entire allowed population once for the
combined batch**, checks every native reading against the accepted baseline,
and refuses any lost verified, exact keyed, verified default or exact default print. Additions, removals,
merges and unequal faces all require stronger review. No exceptions are
automatically approved. Existing validations are never implicitly restarted;
inspect the actual process and evidence before arranging a new attempt.

Only a coordinator may integrate a validated patch into an isolated checkout
of the exact frozen source. Check its grammar hashes against the snapshot and
use `git apply --check` before applying. Run repository gates and affected
families there. Rebase or intervening changes invalidate acceptance and require
integration validation on the resulting combined source. A validated campaign
is evidence for its exact snapshot and allowed population, not other inputs.

## Boundaries and cost

This first lane automates deletions. It deliberately hands semantic composition,
new grammar, changed print inverses and new fixtures to a stronger worker.
The 14-rule trial motivating this workflow was parse-unused but required printing
fixes; a broader audit exposed three default regressions. It belongs outside
the mechanical lane until those dependencies are resolved.

Workers reuse frozen binaries and shared corpus inputs, spend no time compiling,
and run small cohorts instead of repeating whole-corpus gates. The coordinator
amortizes full validation over a batch. Full printing remains necessary: the
current engine does not emit a sound dependency certificate for all default
search decisions. Key lists alone are insufficient, and filtering by original
Oracle words can miss print dependencies. Add trace-based invalidation only
with evidence that it preserves default search, refusal and every reading.
No throughput or speedup is claimed without measurements.

For larger pools, place the campaign on a filesystem with ample space and CoW
support. Workers need private grammar directories rather than complete Git
checkouts. In the measured Btrfs environment, the existing copies already shared
unchanged extents while edits stayed isolated; full CoW worktrees are reserved
for lanes that need source changes or builds. The
[resource probe](retirement-worker-pilot-20261116.md#twelve-worker-resource-probe)
completed twelve workers at two helper threads each. Model-service concurrency
limits are independent of this local process budget.

`worker --rules RULE_A RULE_B --limit 2` reserves a fixed bundle for paired model
trials. Per-helper `commands.jsonl`, per-job elapsed times and per-worker invocation
receipts separate helper execution from agent turnaround. CLI exit zero means
the queue command completed, including jobs marked needs-review; it is not a
semantic or printing acceptance signal. Per-reading print reports, including
Station metadata, require complete coverage and all matching readings to pass.

## Durable model pool

After freezing with twelve slots and two helper threads, and completing the scan,
the external CLI launcher can run twelve model workers independently of the chat
agent-slot limit:

```sh
python3 tools/retire_pool.py /absolute/campaign/epoch \
  --workers 12 --model gpt-6-luna --effort max --validate
```

Run under a retained process manager for a campaign that outlives the terminal.
The pool partitions audited-unused rules into fixed disjoint assignments. It
records prompts, CLI event streams, process receipts, an all-rule inventory and
an escalation queue under `pool/`. The optional `--prior /absolute/prior/epoch`
imports diagnostic references only when all frozen files match. It never imports
acceptance or skips assigned checks. A second launch against an existing pool
fails closed; inspect children and leases before arranging recovery.

The launcher serializes CLI initialization to avoid races installing shared
system skills, then allows model turns to overlap. Each agent's writable root
is the epoch and its private notes directory. Models run the deterministic
worker/report commands; they do not edit shared source or results. Treat their
prose as review notes, not machine evidence. All four print gates are checked
for losses against the baseline; an already missing exact-default spelling does
not become a new regression merely because a worker notices it.

With `--validate`, a complete successful worker phase automatically assembles
provisional rule names and runs full allowed-population validation. Repository
gates, source integration and checkpoint promotion remain coordinator work.
The [shared grammar lane design](retirement-authoring-design.md) describes the
protocol for authoring; the legacy `retire_pool.py` tool does not launch it.

A coordinator can retain checkpoint finalization while the audit runs. First
stage the exact assembled patch in a private clean checkout of the frozen commit,
verify its grammar hashes and run `tools/dev.py check` in a retained user service.
Record that service's exact systemd invocation ID, then run:

```sh
python3 tools/retire_checkpoint.py /absolute/campaign/epoch \
  --checkout /absolute/private/integration \
  --gates-invocation SYSTEMD_INVOCATION_ID \
  --branch retirement/mechanical-batch
```

This coordinator requires all eight repository gates from that invocation,
matching source/grammar/patch hashes, and a successful full audit. It creates a
local branch and commit in the private checkout, rescans the allowed population,
and records an isolated checkpoint plus any newly mechanical candidates. It
never merges into the main checkout or pushes. Missing/failed evidence blocks
promotion; expiration leaves the checks running for inspection. Retrying an
existing checkpoint directory fails closed, including after a partial commit.

A model that exhausts its 900-second budget without a final response has an
operational timeout, not a completed semantic review or an infrastructure-fault
diagnosis. After verifying its terminal receipt, empty final, archived checkout,
process and lease, the coordinator may split an original four-to-six-rule timeout
once into two balanced pending jobs of at most three rules each, against the
latest accepted checkpoint. An operator may instead retry an original
one-to-three-rule timeout once on the latest checkpoint with the same rule
owner and a fresh attempt. Pass `--include-small` to
`tools/retire_timeout_operator.py` when preparing the plan to include these
retries. Apply the rehearsed plan by its SHA-256. The original attempt, archive
and cost history remain intact.
A previously retried author repair can be queued once more when the protected
timeout artifacts identify an interrupted coordinator diagnostic and a fresh
independent report passes the assigned cohort and edited cases. The retry
keeps the same rule owner and runs ordinary worker and integration checks.
When an older campaign prompt sent a worker into the full diagnostic and the
worker timed out during that command, a previously retried repair with a
passing coordinator evaluation can get one attempt under the preview prompt.
The recovery verifies the archived prompt, unfinished command, and passing
report against the same timed-out attempt. The replacement worker supplies
fresh dispositions and the candidate runs through the ordinary acceptance gates.
A preview that included edited-operand checks qualifies for the same retry
when its archived passing report exceeded the worker budget and the worker's
record shows that exact command still open at timeout. The replacement uses the
shorter preview and the coordinator checks edited operands independently.
If a retried repair passes the author checks and fails an isolated integration
trial, one further repair can return to the worker queue. It needs a fresh
independent pass and the retained trial and diagnostic records for that exact
rejected attempt.

A verified singleton mechanical removal with named native or print regressions
can move to an author job scoped to those faces. The original evidence stays
with the terminal mechanical attempt and the author runs fresh gates. Timeout children cannot
use this recovery recursively, and neither the split nor a pending child is an
acceptance; each child must complete the normal review and validation gates.
