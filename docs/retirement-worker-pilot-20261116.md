# Luna Max and Terra High retirement pilot

Three `gpt-6-luna` workers at `max` reasoning ran first, followed by three
`gpt-6.1-terra` workers at `high`. Each worker received the same fixed three-rule
bundle as its counterpart. Both stages used identical frozen grammar, helper
binaries, input data and baseline: 728 explicitly allowed face occurrences,
722 names, including six duplicate-name controls. No grammar patch was applied.

| Measurement | Luna Max | Terra High |
| --- | ---: | ---: |
| Median worker turnaround | 200.10 s | 80.82 s |
| Parallel stage, first start to last finish | 571.25 s | 207.99 s |
| Sum of helper execution time | 200.67 s | 197.28 s |
| Agent tool calls | 49 | 38 |
| Estimated Standard API cost, all three workers | $0.06203 | $0.16898 |
| Provisional passes / review cases | 4 / 5 | 4 / 5 |
| Observed operational mistakes | 2 | 0 |

Terra's median turnaround was about 2.48 times faster at about 2.72 times the
estimated API cost. Helper time was similar; the larger difference was time
outside the deterministic jobs: instructions, inference, tool interaction,
inspection and reporting. This is a small observational pilot over three
different bundles, with Luna running first, rather than a repeated interleaved
engine benchmark. The requested reasoning settings also differ.

| Bundle | Luna turnaround | Terra turnaround | Luna API estimate | Terra API estimate |
| --- | ---: | ---: | ---: | ---: |
| 1 | 200.10 s | 75.00 s | $0.01512 | $0.04607 |
| 2 | 187.48 s | 80.82 s | $0.01402 | $0.06051 |
| 3 | 549.22 s | 182.33 s | $0.03289 | $0.06241 |

## Output and review quality

All nine pairs had byte-identical candidate grammars, cohorts, saved score rows,
native reports and native failure files. Both models correctly distinguished
four provisional passes from five printing regressions. Every diagnostic helper
returned zero; a successful helper invocation did not conceal a lost print gate.
Neither model claimed acceptance or changed source outside its permitted work.

The four passing rule names also assembled into identical patches. One shared
validation of that identical candidate passed all 728 benchmark occurrences,
preserving native readings and all four print gates. This proves the bounded
benchmark result, not full-population integration. The original grammar batch
remains paused and uncommitted.

Luna made one read-only path typo and one exec-handle mistake. The latter worker
monitored the existing process instead of restarting it, but its manually
reconstructed CLI duration was approximate. Primary timing uses assigned-thread
start/completion events; helper and job durations come from the runner's own
clock records. The approximate manual duration is excluded from the table.
Terra made no observed operational mistakes. Both model configurations produced
useful review reports. This pilot tests mechanical execution and reporting,
rather than their ability to design new grammar or semantic proofs.

## Pricing and evidence

Rates were checked against the [public Standard API table](https://developers.openai.com/api/docs/pricing)
on November 16, 2026. USD per million tokens:

| Model | Ordinary input | Cached input | Output |
| --- | ---: | ---: | ---: |
| gpt-6-luna | $0.10 | $0.01 | $0.50 |
| gpt-6.1-terra | $1.00 | $0.05 | $5.00 |

Assigned-thread usage was collected with the existing Vizier usage estimator's
accounting rules and independently matched the Cantrip card-lane estimator for
all six workers. Cumulative notifications are deduplicated, cached input is
discounted, and reasoning is already included in output. Every request was
below the long-context threshold and no cache-write tokens were reported.
These are API-equivalent estimates, not subscription charges; supervisor work,
host compute and service-tier/regional adjustments are excluded.

The ignored `.cache/retirement/model-comparison/sanitized-measurements.json`
contains per-worker tokens, timings, rate version, estimator hashes and artifact
hashes. Detailed allowed-input data, job logs and scoped telemetry remain in
that cache directory. No generated measurement data, corpus or prompt text is committed.

## Changes made from the experiment

- Preserve occurrence order and record/face addresses when names and text repeat.
- Accept complete per-reading print reports, including Station metadata, only
  when every matching reading passes each gate.
- Freeze the workflow runner alongside its inputs, so ongoing campaigns can
  keep running while checkout tooling evolves.
- Persist worker invocation timings independently of an agent retaining its
  exec handle, and expose artifact paths plus a bundled `report` command.
- Add fixed rule assignments and per-helper timing records for paired trials.

Full allowed-population setup now accepts all 32,755 occurrences under 32,749
names. The workflow retains its native-preservation checks and four print gates.
No worker compiles Rust or writes into a shared source checkout.

For mechanical work, the CLI can perform the changes without a model. Luna is
the cheaper option for supervised reporting in this pilot; Terra completes the
same reporting sooner. Reduce manual timing wrappers and artifact hunting with
the generated receipts, and send semantic/printing repairs to a separate lane.
