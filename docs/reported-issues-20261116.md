# Reported issue contracts — 2026-11-16

This change addresses the seven open reports #19–#22 and #31–#33. The issue
bodies and all current comments were reviewed. Work uses the pinned Oracle
words and an explicit, immutable unsealed population; no held-out examples or
new Oracle snapshot are used. Reference changes are ledger amendments U0069–U0076.
U0068 is clarified as wording preservation, not a missing-gameplay correction.

Schema 192 adds the typed relations below. Schema 191 remains an immutable input
epoch and migrates without changing the meanings of existing history selectors.
The implementation owns representation, validation and text round trips; runtime
execution remains a separate consumer responsibility.

## One mana replacement with eligible outputs: #19

`EventChange::ReplaceManaChoices` is the complete, single-effect body of one
mana-production replacement. Each `ManaReplacementOption` contains a source
predicate and a concrete mana type. Evaluate those predicates against the
proposed mana source's current, text-adjusted characteristics. The player tapping
that source chooses one eligible output, and the entire produced quantity becomes
that type. An empty eligible set makes the replacement inapplicable. Matching two
options does not create two replacements or permit this replacement to apply twice.

Applicability is independent of the produced quantity, so hypothetical mana
planning and actual production use the same eligible-output callback. Other
replacement effects keep ordinary replacement ordering. The validator permits
one to 64 concrete options only under a complete mana-production replacement;
side effects, conditional bodies, unrelated events and unresolved chosen colors
are refused. A singleton fixed-output sentence retains its existing fixed form.

U0072 gives Naked Singularity and Reality Twist one replacement each, preserving
their separate cumulative upkeep abilities. Their shared list grammar owns one
option list and prints edited predicates and colors from that list. The current
0.2.14 full face export was also checked: both faces are explicitly refused at
their cumulative-upkeep reminder, so it supplies no contrary supported grouping.
The frozen Source still contains the separate replacements described in #19.
A shared, exact cumulative-upkeep reminder production now admits both raw
Oracle faces; altered reminder effects are refused rather than ignored. The independently
reviewed execution capability is documented in
[Cantrip PR #3007](https://github.com/cantrip-rs/cantrip-rs/pull/3007).

## Linked creation identity: #20

`ObjectFilter::CreatedWith(object)` names the current consuming ability's linked
creating text and acquisition on that exact object incarnation. A consumer must
capture that acquisition when the ability is obtained; finding an arbitrary
creating ability of the same source object is insufficient. Separately acquired
copies and unrelated granted abilities never share the relation.

The relation can include nontoken objects put onto the battlefield. Tetravus's
selector therefore remains `Token ∩ CreatedWith(Source)`. U0073 replaces the broad relation on Tetravus, Arcane Artisan, Dual Nature,
Saproling Burst and Tombstone Stairwell, retaining the actual payment and creation
receipts. Faerie Artisans uses the same acquisition-bound creation population and
excludes the whole current Created receipt, even if replacements create several
tokens; neither a source exclusion nor an index-zero exclusion is sufficient.
A deferred program captures its enclosing linked text/acquisition when created,
rather than searching for a new link when the delayed trigger later resolves. `MadeBy` remains broad, and `Created(result)`
still refers to a receipt from the current resolution. Neither is an alias for
the new linked relation.

The later #20 follow-up identified two remaining `MadeBy(Source)` references
on Dance of Many: its token-removal instruction and token-leaves event.
U0184 changes those two relation discriminants to `CreatedWith`, preserving
their Token intersections and source operands. The shared "the token" grammar
phrase uses that acquisition-bound relation in both positions. Generic
`MadeBy` and creation receipts keep their separate contracts; this correction
uses the existing schema-192 model.

## Historical deaths and first destinations: #21

`ObjectSet::FromHistoryDestinations { event, window }` matches the historical
zone-change event using its departing-object view and then projects the first
public destination incarnation. That object contributes only while it remains
in the destination produced by that event. Leaving and returning never revives
the old link. The history window is queried when the expression is evaluated,
including qualifying deaths after the end-step trigger went onto the stack.

Krovikan Vampire uses the same declared death predicate in its intervening
condition and resolving query. Creature and damage-history tests apply to the
departing battlefield incarnation, so an animated noncreature card is handled
through its pre-death view. The return uses the projected graveyard incarnation.
Its existing delayed sacrifice retains the complete successful return receipt.
U0074 records that correction. The competing reading that moved the Vampire
itself is rejected by an explicit antecedent proof; no operand is retargeted.

`FromHistory` keeps its existing subject projection. The new selector requires
an explicit zone-change matcher into a public zone and validates the history
window and all references. It does not flatten its predicate into a current
graveyard query.

## Entry-sacrifice history: #22

`ObjectSet::SacrificedAsEntered(object)` retains historical views of the objects
actually sacrificed by the linked entry ability for that exact incarnation and
acquisition. Selection alone, unrelated later sacrifices, failed entry attempts
and earlier incarnations do not contribute. Copies cannot inherit another
object's noncopiable entry witness.

Matching and aggregation use the sacrificed objects' historical characteristics
while interpreting the predicate through the consuming source's current words.
U0075 replaces Wood Elemental's unanchored `Earlier` count with this relation in
both characteristic-defining stat operands. Changing Forest to Island changes
the predicate, rather than reading an entry-time tally or a chosen number.

Dracoplasm is a control. Its stat-setting instruction is part of the entry
replacement itself and already sums the actual local sacrifice receipt's Before
views. That correct local receipt is retained. It is not changed into a live CDA.
Independent runtime evidence for the entry witness is in
[Cantrip PR #3006](https://github.com/cantrip-rs/cantrip-rs/pull/3006).

## Recorded Exile arrivals: #31

U0069 adds an Exile qualification to Mimeoplasm's `Transition(move, After)`
creature count, preserving its result identity and multiplier three. Matching
that receipt examines recorded arrivals: one Exile arrival and one redirected
Hand arrival contribute one creature, while a later move does not erase the
recorded Exile arrival. A choice receipt or a live zone query cannot replace it.
The grammar's shared entry/exile/count production now emits that qualification.

## Intrinsic basic-land mana: #32

Intrinsic derivation is authoritative. A land's basic land types supply its mana
abilities even when no mana line is printed. The written `Card.abilities` list can
retain a plain printed manifestation for exact wording, but it denotes that same
intrinsic ability rather than a second activation. Unprinted intrinsic abilities
are not materialized in the written list.

The owned `intrinsic` API provides the operational boundary:

1. Classify the face's printed/base definitions before applying copy exceptions
   or type-changing effects. `nonintrinsic_printed_abilities` removes only exact
   plain manifestations proved by those base characteristics.
2. Derive one intrinsic output per current basic land type using `basic_land_mana`.
3. Apply ability grants and removals in their normal layer. Independently granted
   abilities and acquisitions remain distinct; removal effects can remove the
   derived intrinsic abilities too.

The classifier requires the land type, a sole tap-symbol cost, no limits or
targets, one fixed mana of the matching color for the controller, and no other
action or spending restriction. It never coalesces arbitrary equal operations.
Reminder and label metadata retain that same printed definition and do not
create another activation. Scope restrictions and grants remain distinct.
Validate the face before meaning comparison. `canonical::semantic_card` supplies
the card-context normal form; ordinary `canonical::card` preserves wording.
Ability-only comparison has no card-type proof and does not remove these lines.

U0068's five Gates retain their printed manifestations. The five basic land type
controls compare identically with and without those printed lines, while
nonlands, merely Basic objects, different outputs, modified activations and
independently granted abilities remain distinct. This is a contract clarification
and wording choice, not evidence that the upstream Source omitted gameplay.

## Producer consistency with scope proofs: #33

U0070 removes five redundant wrappers on Gustha's Scepter, Northampton Farm,
Vault 13: Dweller's Journey and Ugin's Labyrinth. A live `Zone` query already
guarantees its exact zone. The bounded structural reduction retains the query's
player, linked-object predicates, filters and origin annotations. Unknown fields
remain invalid. It never removes a zone qualification from a transition,
observation, selection, top-of-library take or arbitrary object set.

U0071 retains Chandra's already-proved singleton movement result while removing
its vacuous Any filter. Its observation amendments cover 49 proved immediate
branch moves on 32 explicitly named allowed faces. No new index-zero assumption is introduced. Esper Maduin's
immediate conditional move names its explicit reveal observation. The reveal's
library player is preserved; no intervening effect can borrow that proof. Closed
observed branches remain separate from unpaid reveal/move branches, which have
provided no observation receipt. No global live/observed equality alias is added.
The owned reduction requires an adjacent one-card Reveal and If, the condition's
own matching observation read, and the first effect of each participating branch
to move that identical library query. It stops at intervening effects and
independent frames. The bounded inverse reconstructs a spelling view from the
current reveal operands, and binding-site provenance is cloned or merged explicitly.
A shared print inverse restores a declared Matching wrapper only when its filter
is proved Any; nonvacuous predicates remain required.

## Evidence and validation

The focused fixture contains 24 explicitly allowed reported/control faces.
Tombstone Stairwell's native card-context print gap also reproduces with the
immutable 0.2.14 helper. Its new linked meaning and sole reading remain checked;
the full prepared-reference audit checks its wording separately.
The observation regression checks all 32 native reading sets and 31 native
wording/default prints. Hans Eriksson's native card-context printing gap was
reproduced using the immutable 0.2.14 helper; its expected meaning remains checked,
and the separate prepared-reference print audit still checks its exact/default
wording. The registered mana family checks two raw reminder faces and two
altered-reminder refusal controls.
Regressions exercise corrected native readings, encoded relations, exact wording,
verified defaults, edited operands, intrinsic comparisons and schema-191 origin
migration. The full unsealed audit remains separate from those family checks.
The singleton receipt reduction also accepts a vacuous filter already removed
from CountObjects, retaining its existing cardinality proof. Ardyn, Break
Expectations and Corpse Appraiser keep their prior supported meanings. U0076
records that same proved convention for Inscribed Tablet's at-most-one selected
card arrival and the successful single-target Before receipts on Kaervek's Purge,
Kalitas and Saw in Half. It preserves result IDs, phase and surrounding negation;
zero successful objects still fails the positive test. Multi-object receipts and
replaceable one-card mill requests retain their batch forms.

The old two Living Lore readings differ only by a vacuous Matching(..., Any)
wrapper around the same linked-card singleton. Their native canonical merge is counted separately from the explicit
replacement of incorrect historical or linked-creation readings.

Record completed gate and audit results with the final implementation; a reference
match alone is not proof of the historical or replacement contract.

Rules basis: [Wizards' 2026-11-06 Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt),
especially 305.6, 400.7, 607.2c, 607.2g and 607.5. The immutable input rules file
has SHA-256 `524f5195a1e5f54bb13c735edf399ac717a4458e2713f398d1a3e6877186b829`.
