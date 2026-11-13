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
