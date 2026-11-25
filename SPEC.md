# Oracle Grammar specification

## Purpose and ownership

Oracle Grammar maps Oracle text to structured rules IR and maps that IR back
to Oracle text. The repository owns the grammar and the IR
contract. MTGish supplies an independently versioned validation reference.
Rules execution and game-state simulation are outside this repository's scope.

`crates/oracle-ir` owns the typed model, closed vocabulary, traversal and wire
schema. `crates/oracle-ir-contract` owns validation, canonicalization, binding
contracts and the compact authoring codec. `src/model.rs` is the frontend's
single import boundary. MTGish Source lowering and execution backends are not
compiled model dependencies. Ownership changes preserve semantic, ambiguity,
refusal and printing checks.

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

Gift spell text specializes one bound spell into promised and unpromised
resolution branches, each with its own target announcement. The gift recipient
remains GiftedPlayer, independently of the caster and other targets. Only pure
promise predicates are projected; remaining conditions and descendant ability
frames keep their identities. Permanent gift headers retain their separate
entry/trigger abilities. A bare gift header beside an independent root spell
cannot represent the spell's promise alternatives. See [Gift branches](docs/gift-branches.md).

Own-spell Source.SpellX queries with current-or-last-known information lower
to AnnouncedX, consistently with that spell's other casting amounts. This
typed frontend normalization excludes costs, independent abilities, deferred
programs, other objects and captured or historical values. Numeric conditional
operands retain their actual words and complete where-definition. See
[numeric thresholds](docs/spell-x-thresholds.md) for the scope contract and
reference amendment U0033.

Abilities added by a copy exception belong to the copied holder. Token copies
introduce a holder even during the creator's entry; an own-source entry copy
retains its existing source. Literal exiled-card references in an activation
body name its one proved singular exile-payment receipt, with its actual cost
address. They do not select an arbitrary persistently linked card. A proved one-object
activation exile receipt also measures its properties as that card, consistently
with ordinary singleton movement receipts; U0034 normalizes the two reference
records that used sums over those singletons. See
[copiable abilities and actions](docs/copiable-abilities-and-actions.md).

Counter placements offered as resolution payments use the native counter cost
and the actual payment receipt. Payer and counter recipient remain independent;
ordinary optional placements retain their separate program form. The shared
cost composes with retained animations and prints their current operands. See
[counter payments](docs/counter-payments.md).

Preserve distinctions among:

- source, granting object and ability holder;
- actor, controller, owner and chosen player;
- target identity, event subject, result receipt and current zone incarnation;
- payment choice, successful payment and its before/after object reference;
- live quantities, captured quantities and per-object or per-player scope.

Canonicalization may remove irrelevant ordering and identifier spelling only
where the schema defines equivalence. It may not replace identity, change scope,
weaken predicates or turn an unsupported structure into agreement.

A negated object-bound condition compares in one form: `ObjectFilter::Not(Condition(c))`
is `Condition(Not(c))`, since both evaluate `c` once for the same candidate. Only
comparison merges them; each prints from its own form. Selection encodings and
arbitrary live versus observed card references remain distinct. An adjacent
one-card library Reveal and If can use the observation in the first Move of a
pure branch only when the condition already consumes that same observation and
the moved query is exactly the producer's query. Intervening effects and independent
ability frames invalidate the proof; it never escapes the branch join. The print
inverse reconstructs its query from the current producer operands and verifies
the reparse. See [the bounded producer proof](docs/reported-issues-20261116.md#producer-consistency-with-scope-proofs-33)
and [the earlier producer forms](docs/reported-issues-20261115.md#producer-normal-forms-29).

A sole-use scalar capture immediately before a pure Snapshot pump may be inlined:
that effect evaluates both stat operands when it resolves, with no intervening
state change. Count every typed result use, including delayed program captures.
Keep captures used later, nonlocal or choice-dependent expressions, live effects,
and enclosing abilities with descendant Ability scopes. Invalid operands must
remain invalid, and cloned identifier leaves retain their source provenance.

An exact, unrestricted activation cost input naming only its Source can compare
as Source when its directly addressed sacrifice or exile component proves that
original identity. Performed/After histories, choices, nested payments and other
ability frames stay distinct. See [the bounded source input proof](docs/source-input-normal-form.md).

A universal player loop containing only one unrestricted fixed mana action can
use AddMana(All) when its result is unobserved. The actions are simultaneous in
both forms. Keep loops with choices, dynamic amounts, restrictions, ordering or
observed results; preserve invalid duplicate bindings and undeclared operands.
The moved effect retains its result identity and source provenance.

Positive fixed literal mana in one production uses totals by color. Repeated
units and nested literal bundles have the same meaning as one amount per color;
choices, dynamic values, chosen colors, zero amounts and overflow are excluded.
Printing can remove one unit at a time in every available color order, so keys
retain exact symbol order while edited counts print from their current values.

A trailing mana spending sentence qualifies one explicit mana clause, including
its optional choice or both arms of a conditional replacement. The structural
lens accepts only single-instruction Programs of AddMana, Optional and If;
it preserves deciders, recipients, conditions, mana operands and result IDs.
It refuses other actions, scopes, sequential programs and prequalified leaves.
The inverse requires the same nonnull restriction on every alternative before
removing it for the shared sentence. Both directions bound bytes, nodes, depth
and producer count.

Alternative CastSpell payment predicates normalize to one CastSpell with
alternative ObjectFilters. Only nonempty Either lists containing exclusively
CastSpell arms qualify. Mixed payment actions and ability source domains remain
distinct. Whole filters and their typed ID origins move intact after their
single canonical visit. Singular spell phrases can share one cast verb; their
filter union inverse enumerates every nonempty partition, bounded to ten filters,
so exact wording and edited operands do not depend on the original grouping.

A named player's next untap step fixes both the permanent rule's step-player
predicate and its expiration window. Controller-following wording keeps its
separate controller-based rule and duration. A narrow canonical proof can replace
ControllerOf(Affected) with AbilityController only for one Skip rule over a live
current battlefield population explicitly controlled by AbilityController,
with DuringNextUntapStep(AbilityController). Snapshot/fixed or unfiltered pools,
other players/windows and more than 64 conjuncts do not prove that implication.
The replaced roles contain no typed IDs; all other operands and origins remain.

Post-destroy characteristic tests retain the original target, including its
last-known characteristics when appropriate. A performed-action Before receipt
can be empty when destruction fails; it cannot substitute for attempted input.
Direct least-power destruction computes the minimum over the complete printed
population before excluding indestructible choices. An independently instructed
choice keeps its own printed eligibility.

An entering exile instruction is an effect that does as much as possible, with
an explicit completion test; it is not an indivisible payment. Completion and
"exiled this way" quantities use recorded After arrivals filtered by Exile.
Matching a transition's recorded arrival incarnation differs from querying
current zone membership: a replacement redirect does not count, and later
movement cannot erase a recorded exile arrival. Real costs retain their payment
contracts.

A continuous effect on one card target outside the battlefield affects that card
only while it stays in the zone it was targeted in: `@zone_card_members` keeps
that zone on the affected set, whichever production created the effect. Moves
keep the bare target, so "in" and "from" spell the same graveyard target.

All-player shuffle-then-draw wording retains two phases: finish every shuffle,
then bind each drawing player and count that player's Before movement receipt.
Do not use the aggregate total for every player or interleave shuffle and draw.
A separate recipient, such as a mill target, does not redefine the performer of
an ordinary single-player receipt.

Positive reveal conditions count matching observed cards. A nonland branch
negates the type filter inside the observed population; negating the existence
condition would incorrectly enter that branch when nothing was revealed.

An unqualified nonattacker test within an attack trigger uses the current
combat; explicit "this turn" history keeps its full-turn window. A distinct
continuous-control-since-turn-began exemption keeps its own timing contract.

An intrinsic, unconditional chosen-number power/toughness definition uses the
existing characteristic-defining classification and retains its symbolic
formula. It functions in all zones and applies in layer 7a. An undetermined
number uses zero without replacing the formula by a current value. The defining
grammar's context contract keeps ordinary setters, granted abilities and
conditional effects distinct.


A named player's most recent completed turn is HistoryWindow::LastTurnOf(player),
including multiplayer and extra-turn distinctions. It is empty before that
player's first turn, and differs from LastTurn and SinceLastTurnOf. A put-action
history event records the actor; entering under a player's control records the
controller. Reference preparation uses the owned frozen-schema migration before
current-epoch amendments, with both parent and face headers upgraded explicitly.
The immutable source reference and its extraction provenance retain epoch 190;
the owned model and native outputs use schema 193. Frozen reference migrations
and evidence-backed amendments supply the current comparison contract.

Mana replacement choices retain their complete production, eligible concrete
mana types, full quantity and choosing player in `ReplaceManaChoices`. Plain
printed basic-land mana manifestations coalesce with the authoritative intrinsic
derivation; independently granted ability acquisitions retain their identities.

`CreatedWith` identifies the exact creating source incarnation, linked text and
ability acquisition; deferred programs capture that link when created.
`FromHistoryDestinations` applies its historical predicate at departure and
records the first public arrival. An object contributes only while it remains
in that destination; a later move cannot revive the link. `SacrificedAsEntered`
names the sacrifice associated with the exact entry and acquisition. See
[the schema-192 issue contracts](docs/reported-issues-20261116.md).

Recorded zone-constrained transitions, cumulative instantiated-shield receipts and
finite mana-cost spending permissions retain separate typed contracts. See
[the schema-193 receipt follow-ups](docs/receipt-followups.md).

An immediate choice from a completely revealed hand uses the observation
receipt, guarded by Hand and that same owner. The owned proof requires adjacent
Reveal/ChooseObjects instructions over the identical one-player whole hand;
partial observations, different hands and intervening actions remain distinct.
U0035 records the corresponding reference normalizations. A copy action's lexical
other-population excludes the actual copied object, preserving explicit foreign
references. See [nonlegendary predicates and observed hands](docs/nonlegendary-and-observed-hands.md).

A plain exchange Spell may represent its distinct pair as one two-object target
group. The owned proof accepts only two required singleton slots with matching
target-free eligibility, an explicit second-object distinctness test and one
exchange action. Other target uses, metadata and program shapes remain distinct.
U0036 normalizes the six corresponding reference frames. Continuous copy actions
also share native keyword/mechanic copy exceptions and preserve each addition's
ability scope. See [copy exceptions and exchange pairs](docs/copy-exceptions-and-exchange-pairs.md).

Literal copied names use SetName of their current string operand, separately
from retaining a runtime name. Current-ability copy additions keep their native
identity and compose with scoped keyword/mechanic additions, including indefinite
copies. Graveyard target qualifiers share one meaning across their supported
word orders. See [copy details and graveyard qualifiers](docs/copy-details-and-graveyard-qualifiers.md).

## Printing

Printing uses the current operands of an edited or synthesized IR. A saved
Oracle string cannot override a changed operand. Derivation and spelling keys
may preserve surface choices only when they still describe the current meaning.

Printed candidates pass semantic reparse before being accepted as verified
prints. Exact preservation of the original text is a separate gate. A
meaning-preserving canonical spelling need not be byte-identical to Oracle.

Closed literal ability declarations also expose a checked canonical inverse
view, derived from their sole `@sem` meaning. It preserves access to literal
wording after group flattening and result-ID normalization. Holes, symbolic
functions and dynamic token classes are excluded; matching still uses current
operands and verified printing still reparses the complete output. This is an
inverse view, not an AST or reference normalization. See
[closed literal printing](docs/literal-inverse-printing.md).

A declared `AllOf` list of two to four members also prints its canonical
conjunction when the declared pattern does not unify: the folded conjuncts,
`Any` and nested lists included, are dealt back out among the members in any
order, as `@and` does. It is a print-only inverse view; forward meaning is
unchanged and every text still reparses.

When the exact text is unreachable, the print frontier reports the furthest
target token any partial inverse derivation reached and the rules that failed
there, the counterpart of the parse frontier. `score --render` records it on
non-exact rows and `corpus.py print-gaps` clusters them.

## Validation and reference inputs

The model and validation reference have separate provenance. `Cargo.lock` owns
the resolved dependency graph; `pins/model.json` records owned package names,
versions, paths, schema version and the immutable extraction origin. Model
crates are workspace members, without dependency patches or replacement
providers. Model changes are ordinary reviewed repository changes; downstream
consumers pin an immutable repository revision.

`adapters/mtgish/reference.json` records reference provenance, schema and artifact
checksums. Downloads are optional, explicitly invoked and verified before use.
Reference updates do not change the model or invalidate compiler inputs. If an
upstream schema changes, an explicit adapter change or model migration is
required; no implicit coercion is allowed.

The pinned export is never edited in place. `adapters/mtgish/updates.json` holds
the only local changes to reference records: each update is a correction or a
normalization with evidence, naming every face it changes, the IR path, the value
it replaces and its replacement. Updates are written against one reference
revision and apply when corpus inputs are prepared. A missing face, an ambiguous
name, a value that no longer matches or one upstream already adopted fails
preparation, so advancing the reference reviews every update again. Scores report
the faces each update changed. An update is a reviewed change to the expected
meaning, never an equality alias or a scorer exception.

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
Reuse them for runtime syntax edits. `python3 tools/dev.py test` uses the same release
profile and CLI feature set, with no binary links. Keep one routine profile and
feature configuration instead of rebuilding a matrix at each checkpoint. Batch parse-only triage; print and reparse
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
