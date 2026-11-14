# Copied names, current abilities and graveyard qualifiers

Copy exceptions now express literal names and the addition of the current
ability. A name written as the source card's printed name becomes SetName of
that literal string. It is distinct from KeepName, which retains the object's
current name. The inverse checks the current SetName operand against the caller's
card name before using SELF_NAME; an edited literal can instead use the existing
referenced-name terminals. Changing the creator's name cannot replace a copied
name stored in the IR.

The ordinary keyword addition has one semantic declaration, shared by its
standalone phrase and “it has … and this ability.” AddCurrentAbility remains its
native reference to the enclosing ability, alongside the separate scoped keyword
or mechanic definition. It does not embed a recursive duplicate of the ability.
Indefinite copy actions now use the same exception operands as duration-qualified
copies, while preserving Indefinite rather than inventing an expiration.

One target-card production gives the graveyard qualifier both word orders:
“target creature card with mana value X from your graveyard” and “target creature
card in your graveyard with mana value X.” Its single @sem preserves the card
domain, owner, eligibility, count and actual target slot in both directions.
Oracle words remain input; no source text is rewritten.

These changes replace the two specialized Impossible Man and Likeness Looter
productions. Their old declarations hardcoded a copied name or target filter and
left written operands outside the declared meaning. The shared productions print
current names, filters and exceptions and work after another target has already
been introduced.

## Validation

Ten compositional cases cover source and explicit literal names, keywords and
numbered mechanics alongside the current ability, both graveyard qualifier
positions, an earlier target and two incomplete-input refusals. Three frontend
regressions change a copied name to Command Tower, change its creator's name,
and edit the graveyard target's type, mana value and added keyword. Complete
semantic reparse checks every synthesized print.

The frozen allowed-population audit retains reference agreement on all 32,755
faces and every one of the 33,825 native readings, with no missing, added or
merged meanings. All readings are valid/canonical and all source/canonical gold
records validate and are idempotent. The 1,609 affected faces have 1,565 verified
prints, 1,538 exact reproductions and 1,559 verified defaults. No affected face
loses any of those gates. Impossible Man and Likeness Looter gain all three.
Applying this comparison to the accepted full-print baseline and focused
overlays leaves 668 tracked exact gaps: 331 lack a verified print and 337 have
another wording. This was not a fresh full-population printing run.

All eight repository gates pass, including 843 frontend tests, 114 owned-model
tests, 75 Python tests and all compositional families. This batch changes syntax
and removes specialized productions; it changes no owned AST normal form,
reference records, wire schema or pins.

Compositional reference coverage rises by two faces to 20,016. Retiring the two
specialized declarations leaves 11,976 seeds (230 retired cumulatively), with
80 unused, zero at-risk faces and the same four attribution limits. Other unused
productions remain pending separate meaning and printing checks.
