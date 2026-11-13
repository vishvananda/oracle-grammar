# Grammar simplification, 2026-11-13

This work continues the generic `where X is` batch after rebasing onto
`3288e6d` from `origin/main`.

## Grammar changes

Shared card-zone phrases replace separate hand and graveyard value productions.
Counts, distinct names, counters, spell/token history and aggregate statistics
compose as values. Entering with counters accepts its own `where X is` clause.
The preceding batch also narrowed two seeded stat operands that could leave X
unbound or print a whole counted expression in a toughness slot.

The first retirement pass removes 52 seeded productions made unnecessary by
these shared phrases. The audit protects every rule used by the remaining
different face, Satya, Aetherflux Genius. Two unrelated deletion candidates were
retained; edited-operand tests still need useful phrasing beyond the original
corpus inputs. Removing the initial 54 candidates preserved every full-population
parse and reading count, but changed edited prints in two tests. The final 52-rule
pass keeps those tests passing without weakening their assertions. A second pass
retires four shared-X tail rules and the computed-energy payment rule, bringing
the total removed to 57.

## Shared-X lowering correction

Previously, the shared-value recognizer looked only at the first two program
instructions. Parting Thoughts and Savanti Romero, Time's Exile therefore gained
an extra generic reading that reevaluated the counter count for both actions.
Their seeded readings already used one captured number.

The recognizer now finds the two immediate X amounts across the program, while
excluding additional or nested X uses. It inserts `BindValue` immediately before
the first use, preserving earlier instructions. Counter counts share the same
contract as other supported definitions. Printing searches all candidate capture
instructions and accepts only captures at the first use; it cannot move a count
across an earlier draw or counter placement. Independent pumps, delayed uses,
local payments and unsupported object properties keep their existing gates.

Parting Thoughts, Savanti Romero, Curse of Vengeance and Fell Beast of Mordor each
lose one duplicate-evaluation reading. Tests also cover an intervening destroy,
the identity of the counted object, last-known information, changed counter
kinds, and printing/reparsing edited operands.

## Face normalization changes

`U0002` in [`updates.json`](../adapters/mtgish/updates.json) records two reference
normalizations against `d4b172343b06f8887f700b425ebf69a67acf8efa`:

| Face | Existing reference | Normalized form |
| --- | --- | --- |
| Bone Devourer | Duplicate inline counts of +1/+1 counters in the death trigger | Capture once before drawing; both amounts read that capture |
| Necropotence Avatar | Duplicate inline counts of death counters | Place the death counter, then capture once before drawing |

Both explicitly say to draw X cards and lose X life, with one definition of X.
Parting Thoughts and Savanti Romero already use the captured form for their
otherwise analogous unkinded counter counts. The update preserves the pinned
object references (`EventSubject` for Bone Devourer, `Source` for Necropotence
Avatar), counter kinds, `CurrentOrLastKnown` information and original action
result IDs. It allocates a fresh result only for the definition. The pinned
export and reference lock remain untouched. The owned AST already provides
`BindValue` and `BoundValue`; no AST or schema update is needed.

## Satya and created-token values

Satya, Aetherflux Genius previously selected `Result{index:0}` for the mana value
in its delayed energy payment. Its retained reference correctly uses
`Aggregate{objects:Created, property:ManaValue, aggregation:Sum}`: replacement
effects can produce several tokens from the request for one. The new generic
token possessive value constructs that aggregate directly from its current
property and batch reference, and the same declaration supplies its print.
The scalar token-reference guard excludes the competing first-token property.

Implicit `its` requires current token focus. An explicit `that token's` can still
name the batch after a new target. Separate abilities and later unrelated target
pronouns cannot borrow the token. Tests cover the full Satya text, edited power
and toughness values, later actions, explicit token possessives, singular/plural
refusal and semantic reparse. The allowed reference contains no scalar
`ObjectProperty(Result)` tied to a token-creation result, so this audit found no
further face normalization in that shape family.

Energy costs now share one production for symbols, worded energy counters and
computed amounts. Satya and one further face gain compositional reference
agreement. Satya has one reading, a verified default print, exact wording with
a key, and all 23 enumerated candidates retain its semantics. Its reference
and the AST schema are unchanged.

## Validation population

All corpus work uses the manifest's explicitly allowed, hash-verified population
of 32,755 faces. Held-out inputs were not inspected. Input identities:

- Allowed population: `a28ce7928e74232a76a8e1fb5be7ae89c10da93bfd5b82c490ebbd8cae59cc20`.
- Prepared Oracle source: `2281f3d715e8ee6127825154b31202918ef83a9d9a3641a0d23b0163ebea3065`.
- Pinned native reference: `68c1f37c748a09a1b4edc60a110ebad8f75100ae11334060515098bd9b40bd76`.

After `U0002` and the Satya correction, all 32,755 faces remain recognized and
match the reference. Relative to the inherited where-X batch, single-reading faces
increase from 31,527 to 31,531 and total readings decrease from 34,080 to 34,076.
These are semantic and structural results, not timing benchmarks.

With all `m_` and `t_` productions disabled, the 484-face where-X cohort has 32
reference matches, up from 29 in the inherited batch. The full rule audit reports
19,702 compositional reference matches. The two retirement passes leave 12,150
seeded productions. The shared-X correction retires four further tail rules:
`t_operation_then_you_draw_x_cards_and`,
`t_operation_you_draw_x_cards_and_you`,
`t_players_singular_loses_x_life_and_you_gain`, and
`t_you_gain_x_life_and_draw_x`. The energy-payment simplification also retires
`m_pay_an_amount_of_e_equal_to`.

## Shared card populations

One `card_population` declaration now serves counts, card-type counts, greatest
statistics, total statistics and private-zone static subjects. It owns the
plain, filtered, named, cycling and same-name card phrases, each composed with
`card_zone_scope`. Separate count-only and aggregate-only copies of the same
zone queries are removed. Named cards, cycling cards and card types can retain
the same meaning in other supported zones and player scopes.

Tests cover a targeted player's library, all players' hands, all graveyards,
the cast spell's identity in a same-name query, and changing a named population's
graveyard operand to a library before printing and reparsing. All allowed faces
still match; compositional reference matches increase to 19,703. No reference
record or AST schema change was needed for this consolidation.

The pass retires `m_affected_gets_x_x_until_end_of_p2` and
`m_affected_s_power_is_equal_to_the`, bringing the retirement total to 59 and
leaving 12,148 seeded productions. Altar of the Goyf gains the existing explicit
snapshot alternative to its inline Snapshot pump. The two representations
evaluate at the same point; this is a canonicalization followup, rather than a
new Oracle interpretation or a reason to alter its reference meaning.

## Suspect normalization

The Suspect audit covers every occurrence in the allowed pinned reference: 13
scalar `Keyword{action:Suspect{object}}` operations and seven
`KeywordEach{action:Suspect,objects}` operations. The scalar form's external wire
tag is `{"Suspect":{"object":...}}`; inspecting only adjacent `kind` tags misses
it. The two shapes express the same action on one object, and separate grammar
productions created duplicate readings.

`U0003` records all thirteen scalar operations as the set form. Ordinary objects
use `One(object)`; up-to-one targets use their full `Targets(slot)` population.
The record keeps the original actor, Source/EventSubject distinctions, target
slots, moved-object receipt indices and result IDs. The peers already using the
set form include Barbed Servitor, Frantic Scapegoat, Person of Interest,
Convenient Target and Incriminating Impetus.

The owned canonical contract also converts legacy scalar Suspect IR, so edited
and synthesized operands print through the same single grammar declaration.
Canonical target scope retains optional counts across modes and created
invocations without leaking them to siblings. Tests check the canonical form,
actor/object identity, changed attached-object operands, optional populations,
modal targets, idempotence and the mapped origin of each moved target-ID leaf.
This changes canonical AST representation, not its wire schema or game meaning.

The invalid-extra-operand test exposed a separate native-input validation gap:
Serde could silently discard fields. The frontend validation boundary now
requires typed serialization to preserve the supplied native value exactly
before accepting its semantic validation. Unknown or missing native fields are
rejected, and the canonicalizer leaves extra operands present for rejection.

All repository gates pass. All 32,755 allowed faces still match, with 31,548
single-reading faces and 34,059 total readings. Compared with the preceding
population batch, 17 equivalent-shape readings merge and one incorrect reading
is removed. No further mined rules become unused in this pass; the two older,
unrelated candidates remain retained.

The native reading-preservation audit validates and canonicalizes every current
reading, and reports that one intentional removal separately: Case of the Stashed
Skeleton used to have both `Created(result)` and `One(Result{index:0})` readings
for "create a ... Skeleton creature token and suspect it". The first-token reading
is incorrect when creation replacements produce several tokens. Its retained
reference already uses the whole created batch. The grammar correction removes
the wrong lowering; the canonicalizer deliberately does **not** alias a first
member to the whole batch. A regression checks that distinction and prevents the
batch wording from claiming to print an explicitly single-member IR. This is a
lowering correction without a reference-record or AST-schema change.

The 21-face Suspect wording cohort now has one matching reading, a verified
default print and exact keyed wording for every face. The generic while-present
goad clause replaces Hot Pursuit's full-card dependency while keeping its
duration object separate from its recipients. Suspected conditions share their
explicit/contracted subject and use contextual references for negation. Frantic
Scapegoat's event-group action cost uses `@and` so its filter prints independently
of canonical conjunct order.

Those phrase fixes retire `m_object_ref_is_suspected`, `m_it_s_not_suspected` and
`t_when_this_enchantment_enters_suspect_target_creature`: 62 seeded productions
retired in total, 12,145 remaining, and 19,705 compositional reference matches.

## Remaining print issues

The inherited every-tenth-face print results still expose these followups:

- Painsmith and Hazoret's Favor can omit the required `have` after `you may`.
  `may_body` accepts unrestricted `clause_operation`, including subject-led
  continuous effects. Narrowing that path is still needed.
- Cyclops Gladiator can print `you may deals` and nested `where X` definitions.
- Pyromancer Ascension can print `copy that enchantment` for the cast spell;
  typed referent nouns need stronger inverse constraints.
- Brood Sliver has an exact default print, but its enumerated sample has no
  semantic roundtrips (0 of 6). Exact default and enumerated verification remain
  separate measurements.

These printer issues do not justify changing the corresponding face meanings.

## Observed cards and where-X selection

The shared library grammar now attaches a where-X definition to the complete
look-and-select program. Hand selection and optional filtered reveal share their
existing tails. A single X use stays inline; repeated uses capture once before
LookAt and refer to that BindValue for the look count and later choice. The
synthetic repeated-count regression deliberately counts cards in hand, so moving
the chosen cards cannot turn another X use into a second evaluation. A property
definition is also supported at this exact capture point.

Synthetic captures keep LookAt's mark distinct from the synthetic definition,
the ChooseObjects receipt and the Move receipt. Edited definitions are printed
from current operands and reparsed. Inverse matching refuses moving the capture
past LookAt. Scope tests keep independent granted/created abilities separate from
the enclosing X definition.

The existing atomic look production is retained for other library tails. Its
private binding annotation rejects residual owned or inherited AnnouncedX uses:
an explicit local definition must not leave a later X as the spell's casting
value. Independent ability definitions keep their own X. This is a lowering
correction, with no AST schema or reference-record change. Retiring the old direct
look-and-put template also removes its incorrect mixed-definition reading on the
synthetic repeated-X example.

The allowed 47-face look-at-top-X cohort retains 47 equal, single-reading faces,
47 exact keyed wordings and 47 verified default prints. Default wording is exact
for 28; 284 of 385 enumerated candidates reparse semantically. Compositional
coverage for that cohort is 16. The full audit gains 11 compositional matches,
from 19,705 to 19,716. Its four pre-existing attribution-unresolved faces are
unchanged and protected (zero at risk); those are distinct from semantic matches.
The full score remains 32,755 equal, 31,548 single-reading faces and 34,059 total
readings, with no status or reading-count changes against the preceding commit.

Four newly redundant productions are retired:
`m_look_at_the_top_x_cards_of`, `m_operation_put_value_of_those_cards_into`,
`t_look_at_the_top_x_cards_of_7` and
`t_operation_put_value_of_them_into_your_7`. Total retirements reach 66, with
12,141 seeded productions remaining. The two older retained candidates stay.

A follow-up reference audit records another representation difference to prove
and normalize: Keldon Flamesage captures the look quantity, while Emergent
Woodwurm (including its granted Backup ability), Kiora and Thassa's Oracle repeat
the defining expression in later operands. Those larger tails still use seeded
productions. Any normalization must preserve when the definition is read,
optional choices, inherited ability holders, cast receipts and library remainder
identities. No speculative equality alias or reference edit was made in this
batch; the immutable audit evidence is reproducible from the allowed population.

Final repository validation passes all eight gates (fmt, pin, hygiene, Python
tools, clippy, build, families and release tests): 703 frontend tests pass and one
is ignored. The inverse enumerates a LookAt slice's quantity explicitly, rather
than widening all other uses of Take with extra where-clause spellings.

The final every-tenth-face print audit preserves all 3,275 statuses, reading
counts, default texts and exact-wording flags against the pre-batch render rows.
It retains 3,221 verified defaults and 3,196 exact keyed wordings. Enumerated
candidates change from 43,371/31,339 generated/verified to 43,372/31,338: Cream of
the Crop and Nessian Game Warden each have one fewer verified beam candidate,
while The Mana Rig gains one. Failed candidates are discarded by semantic reparse;
these counts do not change the verified default or exact-wording results.

## Larger look programs and reference normal forms

The library grammar now shares battlefield selection, casting from observed
cards, top/bottom selection, optional look-and-select and restoring the looked-at
cards. Exiling a selected card and casting it after reordering has three lexical
pieces: each introduces its results at its own words. Introducing the cast result
at the earlier exile phrase had made the remainder refer to the reorder rather
than the actual departure; the separated declarations preserve that receipt.

Repeated look quantities can be captured across a later numeric comparison.
The shared snapshot gate admits the observed card-handling sequence, an optional
cast with no further X use, and an If comparing X whose branches introduce no
further X uses. Other repeated look definitions refuse capture instead of falling
back to repeated evaluation. Independent abilities, local payments, loops and
unsupported later X owners retain their own contracts. Regression tests cover
this refusal, the actual exile incarnation after reordering, edited captured
properties, live library size after reordering, and separate Backup holders.

`U0004` normalizes three faces / four programs to the existing Keldon Flamesage
capture form:

| Face | Definition and uses | Scope preserved |
| --- | --- | --- |
| Emergent Woodwurm | Capture power before looking; reuse it in the permanent-card mana-value test | Both the own attack trigger and Backup-granted trigger retain their respective holder |
| Kiora, Sovereign of the Deep | Capture the triggering spell's mana value before looking and selecting a cast | EventSubject and the actual observed-library population |
| Thassa's Oracle | Capture devotion before looking; reuse it in the later comparison | The later library-count operand stays live after reordering |

Updates preserve the original raw property references/information policy, all old
result IDs, groups, optional choices and movement/casting operands. Only a fresh
BindValue result is allocated. No native alias equates arbitrary live expressions
across state changes. The existing AST already supplies this capture; schema 190
is unchanged.

Spell adjective lists now share a recursive comma list rather than stopping at
three items. Capital-marked subtype spelling is shared with other subtype lists.
The explicit `you cast ... from your hand` event proves the hand belongs to that
spell's caster; third-party spell filters do not borrow that proof.

Computed numeric conditions share comparison words. Direct object-stat wording
keeps the existing ObjectMatches/Property contract, including the transparent
`its power` / `its toughness` annotations. It does not introduce an additional
scalar ObjectProperty reading of that same wording. This is a grammar lowering
restriction, not a global equivalence between native scalar expressions and
object predicates. Old extra scalar-property readings removed from Sage-Eye
Avengers (two) and Shelinda, Yevon Acolyte (one) are reported separately in the
reading-preservation audit; their retained references already use object tests.

`U0005` records a second, independently justified normalization at 22 sites on
22 faces: `Matching(Zone(Shared(Battlefield), kind, filter), InZone(Battlefield))`
becomes that intact inner Zone population. Every object from that current query
already satisfies the repeated membership filter. `oracle-ir-contract` owns the
reduction. It does not remove membership checks on historical/observed sets or
other zones. Extra wrapper fields are retained for validation to reject, and
moving the inner subtree preserves mapped target-ID origins. Tests cover native
validation, idempotence, mapped IDs, different membership filters and printing.
The same redundant literal wrappers are removed from grammar declarations.

Shared zero-count, threshold and numeric parity conditions print the reduced
population directly. A six-case compositional family checks their complete IR,
exact keyed wording and verified default prints. This keeps the normalized
reference printable without relying on old templates that require the wrapper.

The normalized population also exposed two missing shared attachments: recurring
`the end step` triggers, and conditional player prohibitions. These now compose
with the same player-rule ability and numeric conditions. `play lands` is a
player action, independently of object characteristics. The condition-values
family now has ten complete-IR cases, including these attachments, with exact
keyed wording and verified default prints.

### Limited Resources actor correction

`U0006` is a correction, independently of the reference normalizations above.
The entry trigger says each player chooses five lands they control and sacrifices
the rest. Its reference already collects choices from all players, then performs
one sacrifice operation on the aggregate Unselected receipt. That operation
incorrectly named only One(AbilityController). The corrected actor is All.

The correction preserves the original two-instruction order: all choices precede
the grouped sacrifice. Moving the sacrifice inside the per-player choice loop
would change that order. The choice populations/controller filters, counts,
receipts, result IDs and trigger subject remain unchanged. The existing seeded
clause's semantic declaration is corrected to the same actor set. A regression
checks the actors, ordering, receipt and printing an edited keep count. The AST
already supports PlayerSet::All; no schema or vocabulary change is necessary.

### Validation and retirement in this batch

The full allowed population remains 32,755 recognized/reference-equal faces.
Single-reading faces increase from 31,548 to 31,549; total readings decrease from
34,059 to 34,056. All eight repository gates pass; frontend tests report 716
passed and one ignored. The ten-case condition-values family checks native IR,
compositional recognition, exact keyed wording and verified default printing.

The final rule audit reports 19,757 compositional
matches, up 41 from 19,716. Its four pre-existing attribution-unresolved faces
remain protected with zero at risk. Three retirement passes remove 4, 18 and 16
newly redundant rules: 38 this batch, 104 cumulatively, with 12,103 seeded rules
remaining. The two older retained candidates are still kept.

The native audit validates all 32,755 source and canonical reference records;
canonicalization is idempotent. Every one of the 34,056 current readings is valid
and canonical. Preservation is deliberately reported separately: the strict
preservation command exits nonzero for seven old readings on six faces. Four
are replaced by reviewed meanings (Emergent Woodwurm, Kiora, Thassa's Oracle and
Limited Resources); two old scalar-property readings on Sage-Eye Avengers and
one on Shelinda are removed. The report shows those exact seven losses, the
four corresponding additions and no others. It is not an unconditional
preservation pass, and no equality alias conceals these changes.

To make that audit sound across reference updates, the 25 changed faces' old
implicit singleton readings are materialized from the hash-verified pre-batch
prepared reference before comparison. The saved rows, old operands, current gold
and native audit binary have retained hashes under the ignored audit inputs.
Unchanged implicit singletons continue to use unchanged source gold. No held-out
population is involved.

The optional-scope print audit caught one new bad default on Cream of the Crop:
a generic `you may` prefix absorbed the later selection sentence and omitted
`If you do`. The shared optional-program guard now rejects that route for a
look followed by a card choice. Explicit success clauses own their guarded
selection, while one-sentence `look ..., then put ...` / `look ... and put ...`
forms have a shared imperative body. Tests reject the missing-guard wording for
the whole optional IR and verify both single-sentence forms. This correction
preserves every reading in the allowed population. All 107 `you may look` faces
still match, with 96 exact keyed wordings and 106 verified default prints.

The final 79-face affected reference-print cohort has 79 exact keyed prints,
40 exact defaults and 78 verified defaults. Yorvo, Lord of Garenbrig remains a
default-search gap with exact keyed wording available. Enumerated candidates
verify 629 of 892. The 47-face look-at-top-X cohort now has 26 compositional
matches, up from 16.

The final 3,275-face sampled print audit preserves statuses, reading counts and
exact-wording flags: 3,196 exact keyed prints, 2,232 exact defaults and 3,221
verified defaults. Cream of the Crop is the only changed default against the
retained baseline: `those cards` becomes `them`; its explicit `If you do` remains.
Of 43,404 enumerated candidates, 31,366 reparse semantically. These are semantic
and structural measurements; no timing improvement is claimed.

## Immediate Snapshot pump captures

U0007 records ten reference normalizations: Charix, the Raging Isle; Conifer Wurm;
Glister Bairn; Hellkite Igniter; Kitsune Loreweaver; Loxodon Lifechanter; Radha,
Coalition Warlord; Radha, Heart of Keld; Raffine's Silencer; and Sokenzan Spellblade.
Each binds one deterministic scalar immediately before a pure Snapshot pump,
whose stat operands are its only consumers. The existing Snapshot contract fixes
both values when the instruction resolves. These sites therefore use the same
inline expression form already found on other reference pumps. Charix retains
its negative toughness expression; Raffine's Silencer retains both negatives and
CurrentOrLastKnown information. Every expression subtree, affected population,
source, player/target reference, duration and remaining result ID is preserved.
No lowering correction or AST/schema change is needed.

The owned canonicalizer proves the rewrite using the IR type table: there must
be one result definition, and every typed use must be one of the pump's signed
stat operands. Both instructions must decode and serialize without dropping any
operand. Only deterministic expressions without result references are allowed;
free Candidate and Affected references are excluded, while locally bound filter
candidates retain their query scope. It handles one Pump change with Snapshot
evaluation and constant or same-capture stat operands. It preserves unknown
fields for validation, checks constant negation, and retains ID origins when a
query is cloned into both stats. A final canonical walk renumbers surviving
results. Schema traversal has explicit work and depth bounds.

Gloom Ripper and Monoist Circuit-Feeder retain their capture: a second continuous
effect uses it. Myr Battlesphere retains its capture for later damage. No capture
moves across another action, and delayed-program reads count as later uses.
Live and LiveObjects evaluations remain separate. Enclosing abilities containing
descendant Ability scopes are conservatively excluded so that shadowing/captures
and raw numeric label spelling cannot change the normalization decision. Mode
labels are renumbered before the proof; reduced authoring forms may conservatively
retain a capture, then reach the same canonical form.

Defined bonuses now compose as complete static/perpetual change lists. The
shared grammar requires a written numeric X and substitutes its definition over
the whole list, including earlier bonuses. The unannotated list recursion stays
separate so a trailing definition cannot leave an earlier X unbound. Independent
X/Y stat definitions compose their own signed operands directly, without adding
an AnnouncedY AST variant or sharing the two values. Eight synthetic family cases
check native meaning, exact keyed and default semantic printing, two bonuses
sharing X, both signs, different populations, attached-host identity, and missing
or unused definitions. Three affected families pass all 31 cases.

Five seeded productions retire: `m_gets_x_value_where_x_is_value_2`,
`m_gets_x_x_where_x_is_value`, `m_gets_x_y_where_x_is_value`, and
`t_each_other_filter_phrase_you_control_that_s_3`, and
`m_operation_and_affected_static_changes`. The cumulative retirement is
109 rules, leaving 12,098. The two protected edited-print productions remain.
The final full rule audit has 19,764 compositional matches (previously 19,757),
with the same four attribution limitations and zero at-risk faces.

After retirement, all 32,755 allowed faces still match the reference. There are
31,681 single-reading faces and 33,920 total readings, compared with 31,549 and
34,056 before this batch. Native canonicalization merges 136 redundant readings
on 135 faces. Strict preservation validates the 2,507 explicit old readings
before normalization, uses unchanged gold for 31,549 implicit singletons, and
reports no lost readings and no additions. All 32,755 source and canonical gold
records validate, canonical gold is idempotent, and all 33,920 current readings
validate and are canonical. The archived validator, frozen rows, old operands
and prepared gold have verified hashes in the ignored native audit manifest.

The 13-face captured-pump cohort retains all reference matches, 13 exact keyed
prints, seven exact defaults and 13 verified defaults. Its three later-use cases
remain captured; ten equivalent pairs merge. The six-face defined-stat cohort
retains all matches, six exact keyed prints, three exact defaults and six verified
defaults. Three of those faces now match compositionally. No timing or allocation
improvement is claimed; the helpers use the quick profile.

The sampled printer exposed an incorrect lowering in the retained
`m_operation_and_affected_static_changes` rule: an arbitrary preceding operation
could manufacture EndOfTurn for a second effect with no stated duration. Its
four allowed reference cards (Rookie Mistake, Sigil Blessing, Skulduggery and
Zealous Persecution) all explicitly lead with `Until end of turn`. A shared
clause for literal bonuses now reads the leading duration and applies that exact operand to both
continuous effects. The old seeded rule retires. Five synthetic cases check
EndOfTurn, UntilNextTurn, EndOfCombat, separate hosts and refusal of the
missing-duration wording. All five original faces in the duration cohort,
including Essence Pulse, match compositionally with five exact keyed prints,
five verified defaults and zero exact defaults. No actual reference face or AST schema needs correction;
the defect was a grammar lowering/printing path for synthesized text.

Compatibility tests now compare distinct canonical native reading sets, retaining
all old fixture operands. Two sacrificed-receipt family counts and the departure
controller fixture count change from two to one because the equivalent pump
capture disappears. This does not remove a distinct supported meaning. The new
schema visitor compares borrowed type names instead of allocating temporary
`Ty` values for every visited node.

Public canonicalization also preserves invalid forged internal origin annotations
for rejection. Only the private mapped-canonicalization pass may strip its own
origin wrappers for the lossless decode check; a public input cannot use those
wrappers to turn an invalid capture ID into removable valid IR. A regression
checks this separately from the successful mapped-ID cloning test.

Final sampled printing: all 3,275 faces still match; 3,164 have one reading and
there are 3,394 readings. Exact keyed prints remain 3,196, exact defaults 2,232,
and verified defaults 3,221. Every status and exact-wording flag agrees with the
retained pre-batch sample. Two defaults change: Kaito's Pursuit now explicitly
prints `until end of turn`, correcting the old invented-duration path's misleading
bare grant; Luxior, Giada's Gift uses `for each counter` instead of a redundant
X/Y formula. Essence Pulse retains its prior explicit-duration default. Of
43,405 enumerated candidates, 31,369 reparse semantically; candidate enumeration
still includes known unverified print alternatives.

Final required checks: all eight `tools/dev.py check` gates pass. The frontend
has 726 passing tests and one ignored test; the two model packages pass their
locked release library tests with their existing ignores. All 70 Python tool
tests, formatting, Clippy, model/reference pin checks, hygiene and every
registered compositional family pass. The latest `origin/main` remains an
ancestor of this checkout; no push was performed.

## Linked card statistics and branch-local definitions

The linked-card phrase family now composes a singular X/Y stat definition and
explicit linked populations with shared numeric expressions. Singular linked
answers sum over matching cards in exile; plural populations such as `creature
cards exiled with enchanted creature` also compose with count, total and maximum.
The explicit anchor remains an operand, independent of the stat recipient.
Edited filters, properties and anchors print from their current values and pass
semantic reparse. Eight new frontend tests and ten runtime family cases cover
these contracts, missing definitions, full input consumption, signed independent
properties, distinct holder/recipient identities and bounded scope detection.
Two standalone draw examples retain their existing inline/captured readings;
this batch introduces no new canonical equality alias for those shapes.

U0008 corrects Phyrexian Ingester's static Pump operands. The old power expression
selected index zero from an unordered unfiltered linked population; toughness
used a different implicit history measure. Both now use live Aggregate(Sum)
expressions over creature cards in exile linked to the source. The triggered
exile, affected object and source identity remain unchanged. The official
[Commander Masters release notes](https://magic.wizards.com/en/news/feature/commander-masters-release-notes)
require current characteristics in exile and exclude noncreature cards.
[CR 607.3](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf)
requires adding variable answers when the linked exile ability has exiled
multiple cards. Existing Aggregate/Zone/ExiledWith types express the correction;
no AST schema or model pin change is needed. This is a reference/IR lowering
correction: a first-member query remains distinct from a whole-population sum.

U0009 corrects Creeping Inn's count by adding the explicitly written Creature
predicate to the same linked exile population. Payment, player, movement,
conditional receipt, capture after successful payment, both bound life amounts
and the independent phasing ability remain unchanged. Its old seeded clause
omitted that predicate and hardcoded a later result address. The shared numeric
rules now supply the count and native capture contract.

The new linked population exposed a competing outer `where X is` derivation
that inlined both life amounts into the paid branch. For supported shared numeric definitions, capture classification now
finds required captures in nested programs with explicit work/depth
bounds. An unsupported outer capture shape refuses; it cannot fall back to
reevaluation. The inner program captures immediately before its first use after
payment. Independent abilities, holders, modes and lexical scopes retain their
own definitions. Forward construction propagates bound failures; inverse search
applies the same guard. Creeping Inn keeps two distinct native payment forms,
strict movement and the offered action, both with one inner capture. They are
not equated by canonicalization.

Four more seeded rules retire: `m_gets_x_y_where_x_is_the`,
`m_the_number_of_cards_exiled_with_object_ref`,
`m_create_x_value_value_black_dalek_artifact`, and
`t_players_singular_loses_x_life_and_you_gain_2`. Cumulative retirement is 113,
leaving 12,094. Compositional matches rise from 19,764 to 19,768. The final rule
audit retains the same four attribution limitations, zero at-risk faces and
only the two protected edited-print rules as unused candidates.

All 32,755 allowed faces match the updated reference; 31,680 have one reading
and there are 33,921 readings. All raw/canonical gold records validate, all
canonical gold is idempotent, and every current reading validates and is
canonical. The eight affected faces retain eight exact keyed prints, four exact
defaults and eight verified defaults; six are single-reading. All 27 affected
runtime family cases pass, as do every registered compositional family and all
eight required repository gates. The locked release frontend suite has 734
passing tests and one ignored test; model-package ignores remain unchanged.

Strict preservation against c5fdc9a intentionally exits nonzero: the old
Phyrexian Ingester and Creeping Inn meanings are replaced by the two recorded
corrections. These are the only two old missing readings. The corresponding
additions are one corrected Ingester reading and two corrected Creeping Inn
payment views. There are no other losses, additions or canonical shape merges.
The 2,630 explicit old operands validate before canonicalization; 31,290 implicit
singletons use unchanged gold. The old implicit singletons for both changed
faces were materialized from the hash-verified pre-correction reference. Frozen
rows, both correction operands, prepared gold, runtime grammar, export updates
and the archived native validator have checked hashes. Do not describe strict
preservation as green or turn these corrections into equality aliases.

No timing or allocation improvement is claimed. The helpers still use the quick
profile, rather than immutable optimized timing baselines.

U0010 normalizes 495 exact current-zone sites on 424 reference faces. A Zone
scope over all players, filtered to the ability controller's same zone, now
uses that controller's zone directly. Only the corresponding redundant InZone
term is removed; all other filters, object kinds, zone names, players, operations
and timing remain intact. Direct membership filters become Any; conjunctions
retain their other terms. Multiple same-zone membership clauses, unknown fields,
different controllers and other selector types are excluded. All matching sites
in the pinned allowed population have been updated. Creeping Inn's strict cost
selection is one such site; its payment meaning is unchanged by this step.

The existing owned `canonical::narrow_zone_scope` already proves this shape
equality. No new runtime reduction or compiler/model change accompanies U0010.
Old singleton operands for 389 additional affected faces were materialized from
the hash-verified c5fdc9a reference, preserving independent old inputs for the
native comparison. This normalization is separate from the two semantic
corrections in U0008/U0009.

The final 3,275-face print sample has unchanged statuses, reading counts, exact
flags and defaults against the retained c5fdc9a sample: 3,196 exact keyed prints,
2,232 exact defaults and 3,221 verified defaults. Its 3,394 readings include
3,164 single-reading faces. Enumeration generates 43,405 candidates, of which
31,369 reparse semantically. These print checks do not claim every generated
candidate verifies, nor do they turn the two corrected meanings into aliases.

The final post-U0010 native audit confirms the same two recorded corrections
and no additional semantic differences. All 495 normalized sites preserve their
meanings, every current reading remains valid/canonical, and all current gold
is valid and canonically idempotent. Final `tools/dev.py check` passes all eight
gates after U0010, reusing the compiled helpers without a compiler/model change.
