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
a key, and 22 of 23 enumerated candidates retain its semantics. Its reference
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
