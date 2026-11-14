# Nonlegendary predicates and immediate hand observations

The shared adjective grammar now gives `nonlegendary` its existing native
predicate, Not(Supertype(Legendary)). It composes with ordinary nouns, qualities,
targets and zones in both directions. The returned-entry clauses use the same
object reference grammar, replacing their separate nonlegendary target rule.
No card name or original text is a printing operand.

## Copied objects establish their own contrast

Enabling the adjective exposed a generic Mirrorweave reading that excluded
Source from the affected population instead of excluding the copied target.
The old accepted reference already had the correct target exclusion. This is a
frontend lowering correction, not a reference amendment or a global identity
alias between Source and Target.

The shared copy actions now resolve the lexical `other` in their population
against their actual copied object. The contextual operation handles only a
direct permanent battlefield population with the grammar's own lexical
other annotations, including the learned literal-source fallback. Explicit references, arbitrary typed exclusions,
other predicates, nested queries and ability frames keep their meanings.
The same @sem declares the affected population, copied operand, duration and
copy exceptions. The inverse matches current operands and whole-ability
semantic reparse remains mandatory. Copy actions with a duration and indefinite
copy actions share this population contract; the indefinite and finite generic copy seeds are retired. The old
nonlegendary-copy seed also hardcoded its copied slot to zero; the shared
action uses the actual slot, including after an unrelated earlier target.
The inverse binder already supplies qualified noun views from the current target
announcement. A `symbol(qualified_object)` preference lets that proved noun reach
the bounded default copy search, so an edited artifact target prints “that
artifact.” Plain references retain their ordinary ranking: after an explicit
source subject a bare “it” can name that source rather than a revealed card.
The Nascent Metamorph control retains its explicit observed-card wording without
widening the search budget.

## Whole-hand choices have one population representation

Lay Bare the Heart also gained a second reading when the new adjective enabled
the generic hand-choice grammar. One reading queried the same live hand that
had just been revealed; the other selected its observed cards, still in Hand
and owned by the same player. These are the same immediate whole-hand choice,
not an Oracle ambiguity. The owned canonicalizer now makes that equivalence
explicit instead of retaining competing native shapes.

The proof requires two adjacent native Effect instructions: Reveal of a
one-player Hand/Card zone with the Any filter, then ChooseObjects from that
identical hand. Typed lossless decoding prevents unknown or missing fields from
supplying a proof. The reveal's result definition must be unique in the owning ability frame;
independent token abilities keep their separate result namespaces.
The eligibility filter, count, chooser and all later selected/discard/exile
identities remain. The new observed reference clones the reveal result's origin;
the owner predicate retains the original hand-query player origins. A final
canonical numbering pass accounts for the newly used result.

Partial observations, different hands, intervening actions and independent
program frames cannot supply this proof. Hand cards belong to the hand's owner
under [CR 400.3](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf).
The shared hand-choice grammar uses the observation form directly, replacing
its duplicate live-hand production. Existing literal seeds still pass through
the same owned canonicalizer. Printing also supplies the equivalent live-hand
view, but only when applying that owned canonicalizer to the complete ability
proves it returns the original observation form. Current eligibility, chooser,
owner and receipts remain operands. Counts on the two synthetic full-hand cases now
expect one canonical reading; partial-hand cases retain their distinct readings.

Reference amendment U0035 records 79 normalized operands on 78 allowed faces.
It replaces only the same-hand query with the guarded observation and preserves
all other raw operands and original result addresses. The model vocabulary and
wire schema stay unchanged. This batch changes an owned semantic normal form
and frontend context lowering, in addition to the ordinary spelling changes.

## Validation

The compositional family covers ordinary targets, multiple qualities, graveyard
cards, returned-entry targets, a copy after an unrelated target, the normalized
hand choice and an incomplete noun refusal. Frontend tests print edited types
and eligibility and check copied-target and observation identities. Owned tests
cover equivalence, idempotence, validity, partial/foreign/intervening boundaries,
unknown fields, duplicate definitions and mapped origin preservation.

The frozen allowed-population audit retains reference agreement on all 32,755
faces. All 33,825 current native readings validate and are canonical. The
previous 33,882 readings reduce to exactly these meanings under the owned
normalizer: 57 duplicate readings merge on 57 faces, with no missing or added
meaning. For 23 changed reference singletons the audit materialized operands
from the old immutable reference before applying the new normalizer. The new
reference therefore cannot substitute a different baseline meaning.

The original 850-face gap cohort now has 512 verified prints, 175 exact keyed
reproductions and 511 verified defaults. Exact wording is newly recovered for
Kiki-Jiki, Mirror Breaker; Reflection of Kiki-Jiki; and Woodland Bellower. All
three already had verified paraphrases. The tracked exact gap falls from 678 to
675: 338 lack a verified print and 337 have another wording. The 484 affected
faces retain 472 verified/default prints and reach 468 exact reproductions;
all 64 Gift controls retain verified, exact and default printing. No cohort
loses any of those gates. These are focused comparisons against the accepted
full-print baseline and subsequent overlays, not a new full-population print run.

Compositional reference coverage increases from 19,978 to 20,002 faces. Three
copy seeds are retired, leaving 11,981 seeds (225 retired cumulatively). The
rule audit reports zero at-risk faces, the same four attribution limits and
74 unused seeds. Previously protected unused productions remain; attribution
alone does not prove a seed is safe to remove from exact/default printing.

All eight repository gates pass: formatting, pin check, hygiene, 75 Python
tests, Clippy, helper build, compositional families and library tests. Library
results are 835 frontend tests plus 108 owned-model tests, with five tests
ignored across the workspace. Edited copy operands, ordinary/revealed copy
controls, native origin mapping and the observed-hand inverse all pass.
