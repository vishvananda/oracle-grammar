# Deletion-only retirement workers

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
