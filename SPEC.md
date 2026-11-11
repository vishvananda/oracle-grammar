# Oracle Grammar specification

## Purpose and ownership

Oracle Grammar maps Oracle text to structured rules IR and maps that IR back
to Oracle text. The repository owns the grammar and is intended to own the IR
contract. MTGish supplies an independently versioned validation reference.
Rules execution and game-state simulation are outside this repository's scope.

The current implementation uses a pinned schema through `src/model.rs`.
That boundary also exposes canonicalization, validation and binding helpers
required by the frontend. This is an explicit compatibility bridge, not yet an
independent local schema. The move to local IR must preserve the existing
semantic, ambiguity, refusal and printing checks.

The intended dependency direction is:

```text
Oracle text -> grammar -> context binding -> local IR -> printer -> Oracle text
                                               ^
MTGish reference -> optional MTGish adapter ------+
```

The adapter translates upstream structures to local IR. Grammar productions do
not depend on upstream Source syntax or apply upstream text rewrites. Reference
corrections and grammar changes are reviewed independently; disagreement alone
is not evidence that either side is correct.

## Priorities

1. Correct rules-relevant IR.
2. Valid text that reparses to the same IR, including synthesized or edited IR.
3. The card's actual Oracle wording.
4. Labels, reminder text and other presentation details.

Presentation work must not conceal a rules-meaning mismatch. Rules-meaningful
headings remain semantic input; decorative labels belong to presentation.

## Grammar and semantic declarations

- `grammar/*.ebnf` and its subdirectories own Oracle terminals and composition.
  New rules use W3C notation: `::=`, juxtaposition and `/* ... */` comments.
- `tabnas-ebnf` reads EBNF into the grammar library IR at load time. The export
  command emits the modules as one standard EBNF document.
- Each production declares its meaning once in an `@sem` comment. Recognition
  and printing read that declaration, with one implementation per direction.
- Syntax modules load at runtime. Lexical and print-model inputs are compiled
  inputs and require a rebuild. A lexical fast path must validate its EBNF
  declaration before use.
- The grammar reads Oracle's words. An upstream rewrite cannot make a different
  input count as recognized.
- Compositional checks disable seeded `m_` and `t_` rules. Seeded recognition and
  reusable phrase coverage must be reported separately.

## Recognition and ambiguity

Success consumes the whole input and constructs schema-valid meaning. Opaque,
recovered or ignored effects cannot claim success.

Every distinct supported semantic reading is returned. Distinct readings never
resolve by first match, iteration order, candidate score or thread completion.
A key may select a named derivation explicitly. A limit must produce an explicit
refusal or truncation status, never a claim of exhaustive success.

Bytes, recursion depth, parser work, rendering work and candidates are bounded.
Invalid inputs and exhausted budgets return errors with useful context. They do
not panic, wrap arithmetic or silently drop required semantic content.

## IR, context and identity

The semantic contract is a typed, versioned model. Closed vocabularies use enums
or validated semantic IDs. Numeric operands retain their domain and structure.
Serialized inputs must pass schema and semantic validation before acceptance.

The current template and binding implementation uses schema-checked JSON values
as its construction substrate, then validates native structures. That is not a
claim that every internal intermediate value is a native Rust enum. Future IR
work should strengthen typed continuity without creating a second interpretation
of the grammar.

Context binding and spelling metadata remain separate. Retained text, labels,
JSON key order and debug names cannot act as hidden semantic operands.

Preserve distinctions among:

- source, granting object and ability holder;
- actor, controller, owner and chosen player;
- target identity, event subject, result receipt and current zone incarnation;
- payment choice, successful payment and its before/after object reference;
- live quantities, captured quantities and per-object or per-player scope.

Canonicalization may remove irrelevant ordering and identifier spelling only
where the schema defines equivalence. It may not replace identity, change scope,
weaken predicates or turn an unsupported structure into agreement.

## Printing

Printing uses the current operands of an edited or synthesized IR. A saved
Oracle string cannot override a changed operand. Derivation and spelling keys
may preserve surface choices only when they still describe the current meaning.

Printed candidates pass semantic reparse before being accepted as verified
prints. Exact preservation of the original text is a separate gate. A
meaning-preserving canonical spelling need not be byte-identical to Oracle.

## Validation and reference inputs

The model and validation reference have separate locks. `Cargo.lock` owns the
resolved dependency graph; `pins/model.json` records the current schema provider,
immutable revision and package identities. Builds use published source without
local patch or replacement overrides.

`adapters/mtgish/reference.json` records reference provenance, schema and artifact
checksums. Downloads are optional, explicitly invoked and verified before use.
Reference updates do not change the model or invalidate compiler inputs. If an
upstream schema changes, an explicit adapter change or model migration is
required; no implicit coercion is allowed.

Release assets are the intended distribution format for validation snapshots
when available. Pin the artifact URL, SHA-256, schema version and producer
revision. A tag alone is insufficient artifact identity. Keep downloaded corpora
and build products out of Git.

Corpus audits use an explicit allowed population and immutable input hashes.
Held-out card text remains sealed during rule authoring. Do not inspect held-out
examples, emit per-card held-out diagnostics or seed rules from those examples.
Normal builds and local regression tests need no corpus.

Report each gate separately:

- complete-input recognition;
- number of distinct readings and ambiguity/refusal behavior;
- structural IR agreement against an independently reviewed reference;
- valid printing and canonical semantic reparse;
- exact keyed preservation and exact default printing;
- independent grammar conformance and compositional coverage.

A complete grammar export is not recognition coverage. Both sides refusing or
being unsupported is not semantic agreement. Metrics must identify their input
population, preprocessing and denominator; prepared-text and raw-Oracle results
are different measurements.

## Rust and repository hygiene

Use safe Rust; `unsafe` is forbidden. Checked arithmetic and explicit numeric
domains preserve behavior across build profiles. Untrusted input must not reach
an unchecked indexing or panic path. Persisted formats use stable schema tags
and versions, not Rust layout, debug output or type-name conventions.

Keep one owner for each responsibility and one production model for fixtures,
custom inputs and ordinary parsing. Avoid duplicate implementations, dead
uncompiled modules, speculative abstraction and alternate success paths.

Rust files target at most 800 physical lines and have a hard limit of 1,500.
Functions target 80 lines and must stay within 200 lines and seven parameters.
Split files along responsibilities. Clippy enforces function limits, and the
repository structure check enforces the file limit.

Commit coherent changes. Keep source and reproducible fixtures in Git; keep
build caches, generated diagnostic output and full corpora outside it. Optional
upstream experiments use isolated checkouts, never another worker's active tree.

## Cycle time and checks

Group related grammar fixes before validating the affected phrase families.
Build diagnostic helpers once per Rust, lexical, model or toolchain change.
Reuse them for runtime syntax edits. Batch parse-only triage; print and reparse
the relevant changed cohort. Reserve full corpus audits for explicit checkpoints.

Required repository checks are formatting, tests, Clippy over all targets and
features, Python tool tests, model-pin verification and structure hygiene.
Changes to rules also run the affected family gates, including refusal and
edited-operand checks where applicable. Do not repeat broad checks after they
pass unless new changes or unresolved evidence justify the work.

## Performance evidence

Use immutable release binaries and inputs. Repeat and interleave comparable
runs and report medians, build profile, population and input hashes. Separate
grammar load, parse, print, reparse and corpus I/O. Report structural work
counters alongside elapsed time; retain CPU, allocation and memory evidence
when investigating a bottleneck.

Allocation instrumentation uses a separate build. Its timings are not comparable
to uninstrumented timings. Quick opt-level-1 diagnostic builds are for iteration;
performance comparisons explicitly optimize the frontend. Optimizations must
preserve meanings, refusal and ambiguity behavior, context binding and spelling.
