# Entry payments and qualified keyword printing

Twenty-five allowed faces share "As this land enters, you may pay ... life.
If you don't, it enters tapped." Recognition already produced the correct
entry replacement, optional payment and negative payment test. Its explicit
inverse declaration named the incoming controller inside an
`@entering_controller` wrapper that already supplies that binding. Native
canonicalization expresses the player as AbilityController, so neither inverse
view could match the redundant declaration. Declaring the lexical controller
once lets the existing wrapper translate in both directions. This changes no
native meaning for these faces and keeps the actual payment result identity.
An edited cost prints its new life amount; an independent payer cannot be
silently replaced by entry "you".

The old legendary-equip seed consumed a `filter_phrase` but ignored it, always
restricting the target to a legendary creature. Its inverse could not recover
that unused phrase. The shared keyword declaration retains both the inherent
creature restriction and the complete written quality. "Equip legendary Goblin"
now retains Goblin, rather than lowering to the same filter as "Equip legendary
Elf". Printed changes use the current filter and payment. The five original
allowed equipment references already have the correct creature filter; their
native readings are unchanged.

The ordinary activation-limit grammar now composes sorcery timing with a
written condition. The old whole-activation seed consumed "a" as an unused
numeric operand; it could accept "as two sorcery" and could not print the missing
operand. The new restriction uses literal "a", and its condition remains an
operand of the current activation. Cabal Inquisitor and Hall of Oracles already
have these two restrictions in their references. The old equip and activation
seeds have been retired, leaving 11,989 seeded declarations.

Both directions still use each production's sole `@sem` declaration. Accepted
prints consume the full generated text and reparse to the requested current
native meaning. No saved Oracle wording supplies operands. The AST schema,
vocabulary, model pin, reference lock and reference amendment ledger are
unchanged by this batch. The Goblin/Elf example is a lowering correction for
the previously ignored quality, rather than a new reference equality alias.

## Validation scope

The affected audit population is 32 explicitly allowed original faces: 25 entry
payments, five legendary equipment and the two compound activation restrictions.
All 32 remain reference-equal with one reading and now have a verified default
and exact original wording. Thirty-one have exact defaults. The regression suite
retains 30 complete entry/equip references, checks edited life costs and payers,
changes a Goblin equip filter to Elf and its cost to five, edits the activation
condition, and refuses the invalid numeric sorcery phrase. The phrase family
also runs without seeded rules.

A separate normalization opportunity remains in the owned model: a schema-valid
Controls condition with minimum three does not currently share a canonical
shape with the CountObjects comparison used for "control three or more".
This batch does not introduce a frontend equality exception for those forms.
A normalization must preserve the controlled population, player, binding and
provenance, and exclude historical or captured populations and invalid fields.

The 850-face original gap audit also recovers The Black Gate, whose entry
payment uses its own name rather than "this land". In that tracked cohort,
verified outputs rise from 469 to 502 and exact wordings from 131 to 164.
There are now 348 faces with no verified output found and 338 with another
verified wording, leaving 686 exact-wording gaps. These are focused cohort
measurements; the complete-population printing refresh of the preceding
checkpoint remains separate. No printing losses are accepted in the affected
cohort, the original gap cohort or the 64 Gift controls.

Full recognition stays at 32,755 reference-equal faces with 31,712 single
readings. Native validation strictly preserves all 33,884 pre-batch readings:
no additions, removals or merges. Source and canonical gold remain valid and
canonical gold is idempotent. All eight repository gates pass, including
818 frontend tests, 102 owned-model tests and 75 Python tests. Frozen helpers,
grammar, input hashes, population slices and per-face results remain in ignored
cache. These development-helper coverage runs make no performance claim.
